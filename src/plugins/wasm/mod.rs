//! WASM plugin runtime.
//!
//! Loads WebAssembly plugins, calls lifecycle exports.
//! Host imports (emit event, storage, config) implemented
//! with closure captures or caller data.
//!
//! ## Plugin Contract
//!
//! A valid WASM plugin must export:
//! - `name() -> (ptr: i32, len: i32)` — plugin name string
//! - `version() -> (ptr, len)` — version string
//! - `author() -> (ptr, len)` — author string
//! - `description() -> (ptr, len)` — description string
//! - `on_init()` — called once after load
//! - `on_event(ptr: i32, len: i32) -> i32` — receive event (JSON), return 0/1 consumed
//! - `on_tick()` — called ~1Hz
//! - `on_shutdown()` — called before unload
//!
//! Optional imports provided by the host (in `tanu` namespace):
//! - `emit_event(ptr, len)` — emit event back
//! - `log(level, ptr, len)` — write to app log
//! - `store_get(key_ptr, key_len) -> i64` — packed (val_ptr<<32)|val_len
//! - `store_set(key_ptr, key_len, val_ptr, val_len)`
//! - `store_delete(key_ptr, key_len)`
//! - `config_get(key_ptr, key_len) -> i64` — packed (val_ptr<<32)|val_len

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::core::traits::Plugin;
use crate::events::Event;
use crate::plugins::PluginContext;

/// Errors from WASM operations.
#[derive(Debug, thiserror::Error)]
pub enum WasmError {
    #[error("WASM: {0}")]
    Generic(String),
}

/// Shared state for host imports, stored in Store context.
struct HostData {
    memory: Option<wasmtime::Memory>,
    storage: Arc<Mutex<HashMap<String, String>>>,
    storage_prefix: String,
    event_tx: Option<crate::events::bus::EventSender>,
}

/// Manages the wasmtime engine, compiles modules, instantiates plugins.
pub struct WasmHost {
    engine: wasmtime::Engine,
    modules: HashMap<String, wasmtime::Module>,
    shared_storage: Arc<Mutex<HashMap<String, String>>>,
    event_tx: Option<crate::events::bus::EventSender>,
}

impl WasmHost {
    pub fn new(event_tx: Option<crate::events::bus::EventSender>) -> Self {
        let mut config = wasmtime::Config::default();
        config.wasm_threads(false);
        config.wasm_simd(false);
        config.wasm_reference_types(false);
        Self {
            engine: wasmtime::Engine::new(&config).expect("wasmtime engine"),
            modules: HashMap::new(),
            shared_storage: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    /// Compile a .wasm file (cached).
    pub fn load_module(&mut self, path: &Path) -> Result<wasmtime::Module, WasmError> {
        let key = path.to_string_lossy().to_string();
        if let Some(m) = self.modules.get(&key) {
            return Ok(m.clone());
        }
        let bytes = std::fs::read(path)
            .map_err(|e| WasmError::Generic(format!("read {path:?}: {e}")))?;
        let module = wasmtime::Module::from_binary(&self.engine, &bytes)
            .map_err(|e| WasmError::Generic(format!("compile {path:?}: {e}")))?;
        self.modules.insert(key, module.clone());
        Ok(module)
    }

    /// Create a plugin instance from a compiled module.
    pub fn instantiate(
        &mut self,
        module: &wasmtime::Module,
        plugin_name: &str,
        _ctx: &PluginContext,
    ) -> Result<Box<dyn Plugin>, WasmError> {
        let host = HostData {
            memory: None,
            storage: self.shared_storage.clone(),
            storage_prefix: format!("plugin.{plugin_name}"),
            event_tx: self.event_tx.clone(),
        };

        let mut store = wasmtime::Store::new(&self.engine, host);
        let linker = wasmtime::Linker::<HostData>::new(&self.engine);

        let instance = linker.instantiate(&mut store, module)
            .map_err(|e| WasmError::Generic(format!("instantiate: {e}")))?;

        let memory = instance.get_memory(&mut store, "memory")
            .ok_or_else(|| WasmError::Generic("no memory export".into()))?;
        store.data_mut().memory = Some(memory);

        Ok(Box::new(WasmPlugin {
            instance,
            store,
            memory,
            name_str: plugin_name.to_string(),
        }))
    }

    /// Discard cached module.
    pub fn unload_module(&mut self, path: &Path) {
        self.modules.remove(&path.to_string_lossy().to_string());
    }
}

/// A loaded WASM plugin instance implementing the Plugin trait.
struct WasmPlugin {
    instance: wasmtime::Instance,
    store: wasmtime::Store<HostData>,
    memory: wasmtime::Memory,
    name_str: String,
}

impl WasmPlugin {
    fn read_wasm_str(&self, ptr: i32, len: i32) -> String {
        if ptr < 0 || len <= 0 {
            return String::new();
        }
        let data = self.memory.data(&self.store);
        let s = ptr as usize;
        let e = (ptr + len) as usize;
        if e <= data.len() {
            String::from_utf8_lossy(&data[s..e]).into_owned()
        } else {
            String::new()
        }
    }

    fn write_wasm_str(&mut self, s: &str) -> (i32, i32) {
        let bytes = s.as_bytes();
        let len = bytes.len();
        let off: usize = 1024;
        let needed = off + len;
        let cur = self.memory.data_size(&self.store);
        if needed > cur {
            let pages = (needed - cur).div_ceil(65536) as u64;
            let _ = self.memory.grow(&mut self.store, pages);
        }
        let _ = self.memory.write(&mut self.store, off, bytes);
        (off as i32, len as i32)
    }

    fn call_str(&mut self, name: &str) -> Result<String, WasmError> {
        let func = self.instance
            .get_typed_func::<(), (i32, i32)>(&mut self.store, name)
            .map_err(|e| WasmError::Generic(format!("{name}: {e}")))?;
        let (p, l) = func.call(&mut self.store, ())
            .map_err(|e| WasmError::Generic(format!("{name}: {e}")))?;
        Ok(self.read_wasm_str(p, l))
    }

    fn call_void(&mut self, name: &str) -> Result<(), WasmError> {
        let func = self.instance
            .get_typed_func::<(), ()>(&mut self.store, name)
            .map_err(|e| WasmError::Generic(format!("{name}: {e}")))?;
        func.call(&mut self.store, ())
            .map_err(|e| WasmError::Generic(format!("{name}: {e}")))?;
        Ok(())
    }
}

impl Plugin for WasmPlugin {
    fn name(&self) -> &str { &self.name_str }
    fn version(&self) -> &str { "0.1.0" }
    fn author(&self) -> &str { "wasm" }
    fn description(&self) -> &str { "WASM plugin" }

    fn on_init(&mut self, _ctx: &PluginContext) {
        let _ = self.call_void("on_init");
    }

    fn on_event(&mut self, _ctx: &PluginContext, event: &Event) -> bool {
        let json = serde_json::to_string(&EventWire::from(event)).unwrap_or_default();
        let (ptr, len) = self.write_wasm_str(&json);
        self.instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "on_event")
            .ok()
            .and_then(|f| f.call(&mut self.store, (ptr, len)).ok())
            .unwrap_or(0) != 0
    }

    fn on_tick(&mut self, _ctx: &PluginContext) {
        let _ = self.call_void("on_tick");
    }

    fn on_shutdown(&mut self) {
        let _ = self.call_void("on_shutdown");
    }
}

// ── Event wire format ──

#[derive(serde::Serialize, serde::Deserialize)]
struct EventWire {
    event_type: String,
    #[serde(default)]
    payload: serde_json::Value,
}

impl From<&Event> for EventWire {
    fn from(e: &Event) -> Self {
        let (et, p) = match e {
            Event::Quit => ("Quit", serde_json::Value::Null),
            Event::Play => ("Play", serde_json::Value::Null),
            Event::Pause => ("Pause", serde_json::Value::Null),
            Event::TogglePlayPause => ("TogglePlayPause", serde_json::Value::Null),
            Event::Stop => ("Stop", serde_json::Value::Null),
            Event::Next => ("Next", serde_json::Value::Null),
            Event::Previous => ("Previous", serde_json::Value::Null),
            Event::Seek(s) => ("Seek", serde_json::json!({"pos": s})),
            Event::SetVolume(v) => ("SetVolume", serde_json::json!({"vol": v})),
            Event::SetShuffle(b) => ("SetShuffle", serde_json::json!({"on": b})),
            Event::SetRepeat(r) => ("SetRepeat", serde_json::json!({"mode": format!("{r:?}")})),
            Event::PlayerStateChanged(s) => ("PlayerStateChanged", serde_json::json!({
                "playing": s.is_playing, "pos": s.position_secs, "dur": s.duration_secs, "vol": s.volume
            })),
            Event::ThemeChanged(n) => ("ThemeChanged", serde_json::json!({"name": n})),
            Event::LayoutChanged(n) => ("LayoutChanged", serde_json::json!({"name": n})),
            _ => ("Unknown", serde_json::Value::Null),
        };
        Self { event_type: et.into(), payload: p }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_wire() {
        let w = EventWire::from(&Event::Play);
        assert_eq!(w.event_type, "Play");
        let w = EventWire::from(&Event::Seek(10.0));
        assert_eq!(w.payload["pos"], serde_json::json!(10.0));
    }

    #[test]
    fn test_wasm_error_display() {
        let e = WasmError::Generic("broken".into());
        assert!(e.to_string().contains("broken"));
    }
}
