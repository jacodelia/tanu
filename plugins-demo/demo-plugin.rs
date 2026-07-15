//! Demo WASM plugin for Tanu.
//!
//! This file demonstrates the WASM plugin ABI.
//! Compile with `wasm32-unknown-unknown` target:
//!
//! ```sh
//! rustup target add wasm32-unknown-unknown
//! rustc --target wasm32-unknown-unknown -C opt-level=s \
//!       --edition 2021 -o demo_plugin.wasm plugins-demo/demo-plugin.rs
//! ```
//!
//! Or use a Cargo project with `crate-type = ["cdylib"]`.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::arch::wasm32;

// ─── Memory buffer for string passing ───
// WASM linear memory starts at offset 1024 for string data.
// This mirrors the host-side STRING_BUFFER_OFFSET constant.

/// Simplified string writing: uses a tiny inline buffer on the "stack"
/// (in WASM linear memory). Real plugins should use a proper allocator
/// or a fixed buffer in an exported mutable global.

// In a real WASM plugin built with cargo, the linker provides malloc/free.
// Here we use a minimal fixed-buffer approach for the demo.

static mut BUF: [u8; 4096] = [0u8; 4096];
static mut BUF_LEN: usize = 0;

unsafe fn write_to_buf(s: &str) -> (i32, i32) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(4096);
    BUF[..len].copy_from_slice(&bytes[..len]);
    BUF_LEN = len;
    (BUF.as_ptr() as i32, len as i32)
}

unsafe fn read_from_ptr(ptr: i32, len: i32) -> &'static [u8] {
    if ptr < 0 || len <= 0 {
        return &[];
    }
    core::slice::from_raw_parts(ptr as *const u8, len as usize)
}

// ─── Exports (must match host expectations) ───

#[no_mangle]
pub unsafe extern "C" fn name() -> (i32, i32) {
    write_to_buf("Demo WASM Plugin")
}

#[no_mangle]
pub unsafe extern "C" fn version() -> (i32, i32) {
    write_to_buf("1.0.0")
}

#[no_mangle]
pub unsafe extern "C" fn author() -> (i32, i32) {
    write_to_buf("Tanu")
}

#[no_mangle]
pub unsafe extern "C" fn description() -> (i32, i32) {
    write_to_buf("Demo WASM plugin for Tanu music player")
}

static mut EVENTS: u32 = 0;
static mut TICKS: u32 = 0;

#[no_mangle]
pub unsafe extern "C" fn on_init() {
    EVENTS = 0;
    TICKS = 0;
}

#[no_mangle]
pub unsafe extern "C" fn on_event(ptr: i32, len: i32) -> i32 {
    EVENTS += 1;
    let data = read_from_ptr(ptr, len);
    // Check if event type contains "Play" or "PlayerStateChanged"
    let s = core::str::from_utf8(data).unwrap_or("");
    if s.contains("Play") || s.contains("PlayerStateChanged") { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn on_tick() {
    TICKS += 1;
}

#[no_mangle]
pub unsafe extern "C" fn on_shutdown() {
    // cleanup
}

// ─── Required: memory export ───
// Rust/Cargo provides this automatically via the linker when
// building as cdylib. We declare a static to force the export.

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    wasm32::unreachable()
}
