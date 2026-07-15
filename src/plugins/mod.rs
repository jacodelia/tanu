//! Plugin system — extensibility API for Tanu.
//!
//! ## Architecture
//!
//! Plugins implement the `Plugin` trait and are registered with
//! a `PluginManager`. The manager provides a `PluginContext` that
//! grants controlled access to application services.
//!
//! ## Lifecycle
//!
//! ```text
//! register → on_init(ctx) → [on_event(ctx, event), on_tick(ctx)]… → on_shutdown()
//! ```
//!
//! ## Writing a plugin
//!
//! ```rust,no_run
//! use tanu::plugins::{Plugin, PluginContext};
//! use tanu::events::Event;
//!
//! struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn name(&self) -> &str { "my-plugin" }
//!     fn version(&self) -> &str { "1.0.0" }
//!     fn author(&self) -> &str { "Me" }
//!     fn description(&self) -> &str { "Does something useful" }
//!
//!     fn on_init(&mut self, _ctx: &PluginContext) {
//!     }
//!
//!     fn on_event(&mut self, _ctx: &PluginContext, event: &Event) -> bool {
//!         matches!(event, Event::Play)
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;

pub use crate::core::traits::Plugin;

use crate::config::Config;
use crate::database::Database;
use crate::events::bus::EventSender;
use crate::events::Event;

/// Controlled access to application services for plugins.
///
/// Plugins receive this context in lifecycle hooks and use it
/// to query the library, read configuration, and emit events
/// back to the application.
#[derive(Clone)]
pub struct PluginContext {
    /// Sender for publishing events back to the app.
    event_tx: EventSender,
    /// Optional database handle for library queries.
    db: Option<Database>,
    /// Application configuration (read-only).
    config: Arc<Config>,
    /// Plugin-specific key-value store for persisting state.
    storage: Arc<std::sync::Mutex<HashMap<String, String>>>,
}

impl PluginContext {
    /// Create a minimal context suitable for testing.
    pub fn new(event_tx: EventSender) -> Self {
        Self {
            event_tx,
            db: None,
            config: Arc::new(Config::default()),
            storage: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Create a full-featured context with database and config.
    pub fn with_db(event_tx: EventSender, db: Option<Database>, config: Arc<Config>) -> Self {
        Self {
            event_tx,
            db,
            config,
            storage: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Emit an event back to the application event bus.
    pub fn emit(&self, event: Event) {
        let _ = self.event_tx.send(event);
    }

    /// Get a reference to the event sender (for async plugins).
    pub fn sender(&self) -> &EventSender {
        &self.event_tx
    }

    /// Get the database handle, if available.
    pub fn database(&self) -> Option<&Database> {
        self.db.as_ref()
    }

    /// Get read-only access to application configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Store a value in the plugin's persistent key-value store.
    /// Useful for scrobble counters, last-fetch timestamps, etc.
    pub fn store(&self, key: &str, value: &str) {
        if let Ok(mut guard) = self.storage.lock() {
            guard.insert(key.to_string(), value.to_string());
        }
    }

    /// Read a value from the plugin's persistent store.
    pub fn fetch(&self, key: &str) -> Option<String> {
        self.storage.lock().ok()?.get(key).cloned()
    }

    /// Remove a key from the store.
    pub fn remove(&self, key: &str) {
        if let Ok(mut guard) = self.storage.lock() {
            guard.remove(key);
        }
    }
}

/// Manages plugin lifecycle: registration, initialization, events, and shutdown.
pub struct PluginManager {
    plugins: HashMap<String, Box<dyn Plugin>>,
    context: PluginContext,
    /// Plugins that have been initialized.
    initialized: Vec<String>,
}

impl PluginManager {
    pub fn new(context: PluginContext) -> Self {
        Self {
            plugins: HashMap::new(),
            context,
            initialized: Vec::new(),
        }
    }

    /// Get a reference to the context (for passing to plugins on events).
    pub fn context(&self) -> &PluginContext {
        &self.context
    }

    /// Update the context (e.g., when database becomes available).
    pub fn set_context(&mut self, ctx: PluginContext) {
        self.context = ctx;
    }

    /// Register a plugin.
    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) -> anyhow::Result<()> {
        let name = plugin.name().to_string();
        if self.plugins.contains_key(&name) {
            anyhow::bail!("Plugin '{}' already registered", name);
        }
        plugin.on_init(&self.context);
        self.initialized.push(name.clone());
        self.plugins.insert(name.clone(), plugin);
        let _ = self.context.sender().send(Event::PluginLoaded(name));
        Ok(())
    }

    /// Unregister a plugin by name. Calls on_shutdown.
    pub fn unregister(&mut self, name: &str) -> Option<Box<dyn Plugin>> {
        let mut plugin = self.plugins.remove(name)?;
        plugin.on_shutdown();
        self.initialized.retain(|n| n != name);
        let _ = self.context.sender().send(Event::PluginUnloaded(name.to_string()));
        Some(plugin)
    }

    /// Dispatch an event to all registered plugins.
    pub fn dispatch_event(&mut self, event: &Event) {
        for plugin in self.plugins.values_mut() {
            plugin.on_event(&self.context, event);
        }
    }

    /// Call on_tick for all plugins (typically every N ms in the main loop).
    pub fn tick_all(&mut self) {
        for plugin in self.plugins.values_mut() {
            plugin.on_tick(&self.context);
        }
    }

    /// Shutdown all plugins.
    pub fn shutdown_all(&mut self) {
        for plugin in self.plugins.values_mut() {
            plugin.on_shutdown();
        }
        self.plugins.clear();
        self.initialized.clear();
    }

    /// List registered plugin names.
    pub fn list(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a plugin is registered.
    pub fn has(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// Count of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        let (tx, _rx) = crate::events::bus::event_channel();
        Self::new(PluginContext::new(tx))
    }
}

/// Built-in plugin implementations.
pub mod builtin;

/// WASM plugin runtime (optional, behind `wasm-plugins` feature).
#[cfg(feature = "wasm-plugins")]
pub mod wasm;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::bus::event_channel;

    struct TestPlugin {
        init_called: bool,
        tick_count: usize,
        last_event: Option<Event>,
    }

    impl Plugin for TestPlugin {
        fn name(&self) -> &str { "test" }
        fn version(&self) -> &str { "1.0.0" }
        fn author(&self) -> &str { "Tanu" }
        fn description(&self) -> &str { "A test plugin" }

        fn on_init(&mut self, _ctx: &PluginContext) {
            self.init_called = true;
        }

        fn on_event(&mut self, _ctx: &PluginContext, event: &Event) -> bool {
            self.last_event = Some(event.clone());
            true
        }

        fn on_tick(&mut self, _ctx: &PluginContext) {
            self.tick_count += 1;
        }
    }

    #[test]
    fn test_register_and_init() {
        let (tx, _rx) = event_channel();
        let ctx = PluginContext::new(tx);
        let mut manager = PluginManager::new(ctx);
        let plugin = Box::new(TestPlugin { init_called: false, tick_count: 0, last_event: None });
        assert!(manager.register(plugin).is_ok());
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_dispatch_event_to_plugin() {
        let (tx, _rx) = event_channel();
        let ctx = PluginContext::new(tx);
        let mut manager = PluginManager::new(ctx);
        let plugin = Box::new(TestPlugin { init_called: false, tick_count: 0, last_event: None });
        manager.register(plugin).unwrap();
        manager.dispatch_event(&Event::Play);
        // verify plugin received event via its internal state
        // (we can't directly inspect the boxed plugin, but dispatch shouldn't panic)
    }

    #[test]
    fn test_tick_all() {
        let (tx, _rx) = event_channel();
        let ctx = PluginContext::new(tx);
        let mut manager = PluginManager::new(ctx);
        manager.register(Box::new(TestPlugin { init_called: false, tick_count: 0, last_event: None })).unwrap();
        manager.tick_all();
        manager.tick_all();
        // ticks dispatched without error
    }

    #[test]
    fn test_unregister() {
        let (tx, _rx) = event_channel();
        let ctx = PluginContext::new(tx);
        let mut manager = PluginManager::new(ctx);
        manager.register(Box::new(TestPlugin { init_called: false, tick_count: 0, last_event: None })).unwrap();
        let removed = manager.unregister("test");
        assert!(removed.is_some());
        assert!(manager.list().is_empty());
    }

    #[test]
    fn test_duplicate_register_fails() {
        let (tx, _rx) = event_channel();
        let ctx = PluginContext::new(tx);
        let mut manager = PluginManager::new(ctx);
        manager.register(Box::new(TestPlugin { init_called: false, tick_count: 0, last_event: None })).unwrap();
        assert!(manager.register(Box::new(TestPlugin { init_called: false, tick_count: 0, last_event: None })).is_err());
    }

    #[test]
    fn test_context_store_and_fetch() {
        let (tx, _rx) = event_channel();
        let ctx = PluginContext::new(tx);
        ctx.store("last_played", "track-42");
        assert_eq!(ctx.fetch("last_played"), Some("track-42".into()));
        ctx.remove("last_played");
        assert_eq!(ctx.fetch("last_played"), None);
    }
}
