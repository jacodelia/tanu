# Architecture Decision Record — Tanu

## ADR-001: Runtime — tokio

**Decision:** tokio (multi-threaded, work-stealing).

**Alternatives considered:**
- `async-std` — smaller ecosystem, fewer crates compatible.
- `smol` — minimal but less battle-tested for complex apps.
- No async — simpler but blocks UI during I/O.

**Rationale:** tokio dominates the async Rust ecosystem. `ratatui` + `crossterm` event loop runs on a single thread; tokio handles background work (audio decoding, file indexing, DB queries) on its runtime. We use `tokio::sync::mpsc` for inter-component channels because they integrate natively with `async/.await`.

**Trade-off:** Adds complexity vs. single-threaded. Mitigated by clear component boundaries and `#[tokio::main]` at the entry point.

---

## ADR-002: TUI backend — ratatui + crossterm

**Decision:** ratatui for widget rendering, crossterm for terminal I/O.

**Alternatives considered:**
- `tui` (deprecated) — ratatui is the maintained fork.
- `termion` — Linux-only, no Windows support.
- `ncurses` bindings — unsafe, C dependency, poor Rust ergonomics.

**Rationale:** ratatui is the de facto Rust TUI framework. crossterm provides cross-platform terminal control and mouse support. Pure Rust, no C dependencies.

---

## ADR-003: Audio backend — symphonia + rodio (decoupled)

**Decision:** symphonia for decoding, rodio for output. Abstract behind `AudioBackend` trait.

**Alternatives considered:**
- `kira` — good API but less mature output backends.
- `cpal` directly — too low-level.
- `gstreamer-rs` — heavy C dependency.

**Rationale:** symphonia is pure Rust, decodes MP3/FLAC/AAC/Opus/WAV. rodio wraps cpal with a simple API for playback. Trait abstraction over `AudioBackend` means swapping `rodio` for `kira` or a raw `cpal` pipeline requires changing one module.

**Architecture:**
```
Decoder (symphonia) → Samples (Box<dyn SampleSource>) → AudioBackend (rodio/kira)
```

---

## ADR-004: Database — SQLite via rusqlite

**Decision:** rusqlite (synchronous) wrapped in `spawn_blocking`.

**Alternatives considered:**
- `sqlx` — async SQLite requires `libsqlite3-sys` with `feature = "sqlite"`. rusqlite is more mature for SQLite specifically.
- `sled` — embedded K/V store, no SQL querying for complex library queries.
- `redb` — pure Rust but no SQL.

**Rationale:** Library metadata is relational (artists, albums, tracks, playlists). SQLite is embedded, zero-config, and handles 100k+ tracks easily with proper indexing. rusqlite is the most battle-tested SQLite crate. Wrapping in `spawn_blocking` prevents blocking the async runtime.

**Trade-off:** rusqlite is synchronous. We use `tokio::task::spawn_blocking` for all DB calls.

---

## ADR-005: Metadata — lofty

**Decision:** lofty for reading audio file metadata.

**Rationale:** Pure Rust. Supports MP3 (ID3v1/v2), FLAC (Vorbis comments), MP4, Opus, Vorbis, WAV. Faster than `id3` + separate crates per format.

---

## ADR-006: Communication — Event Bus (mpsc channels)

**Decision:** Multiple `tokio::sync::mpsc` channels, one per component, with a central `EventRouter`.

**Alternatives considered:**
- Single global `broadcast` channel — all components receive all events, wasteful.
- `actix` actor framework — heavy, opinionated.
- Callbacks via `Arc<dyn Fn>` — hard to debug, no backpressure.

**Rationale:** `mpsc` channels provide typed, directed communication. Each component exposes a `Receiver<T>` for its specific event type. An `EventRouter` dispatches from source channels to destination channels. This is explicit, testable, and supports backpressure.

**Channel topology:**
```
InputHandler → EventRouter → [Library, Player, UI, Commands, ...]
Library → EventRouter → [UI, Database, ...]
Player → EventRouter → [UI, ...]
UI → EventRouter → [Commands, Player, Library, ...]
```

---

## ADR-007: Error handling — thiserror + anyhow

**Decision:** `thiserror` for library crates, `anyhow` for application-level error propagation.

**Rationale:** `thiserror` provides typed, matchable errors for internal APIs. `anyhow` simplifies error handling in the main binary and commands where exact error types don't matter.

---

## ADR-008: Configuration — serde + TOML

**Decision:** Multiple TOML files (`config.toml`, `bindings.toml`, `theme.toml`, `layout.toml`) with hot-reload via `notify`.

**Rationale:** TOML is human-readable, well-supported by serde. Separate files allow themed configs to be swapped independently. Hot-reload uses the same `notify` watcher as library indexing.

---

## ADR-009: Plugin system — trait-based initially, WASM later

**Decision:** Define plugin API as Rust traits. `Plugin` trait with lifecycle hooks. WASM via `wasmtime` planned for post-v1.0.

**Rationale:** Trait-based plugins are the simplest path to extensibility. They compile into the binary. WASM enables runtime loading and sandboxing — important for community plugins — but adds complexity. Defer WASM.

**Plugin trait sketch:**
```rust
trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn on_init(&mut self, ctx: PluginContext);
    fn on_event(&mut self, event: &Event);
    fn on_shutdown(&mut self);
}
```

---

## ADR-010: Widget system — trait-based with differential rendering

**Decision:** All UI widgets implement `Widget` trait. Each widget owns a `Rect` and a dirty flag.

**Alternatives considered:**
- Immediate-mode rendering — simpler but redraws everything every frame.
- ratatui's built-in `Widget` trait — insufficient for interactive stateful widgets.

**Rationale:** Differential rendering avoids redrawing unchanged regions. Each widget tracks `is_dirty` and its last `Rect`. The `Screen` compositor only calls `draw` on dirty widgets whose area intersects the render surface.

**Widget trait sketch:**
```rust
trait Widget {
    fn id(&self) -> WidgetId;
    fn rect(&self) -> Rect;
    fn set_rect(&mut self, rect: Rect);
    fn is_dirty(&self) -> bool;
    fn handle_event(&mut self, event: &Event) -> EventResult;
    fn render(&mut self, frame: &mut Frame, area: Rect);
    fn focus(&mut self);
    fn blur(&mut self);
}
```

---

## ADR-011: Library indexer — incremental, background, notify-driven

**Decision:** `walkdir` for initial scan, `notify` for file system changes, SQLite for persistence. Background task on tokio.

**Rationale:** Initial full scan with `walkdir` builds the library DB. `notify::Watcher` subscribes to music directory changes and enqueues incremental updates. Both run on a dedicated tokio task spawned at startup.

**Pipeline:**
```
File system event → notify watcher → mpsc channel → Indexer task → SQLite INSERT/UPDATE/DELETE
```

---

## ADR-012: Search — FTS5 via SQLite

**Decision:** Use SQLite FTS5 extension for full-text search.

**Alternatives considered:**
- In-memory `fst` or `tantivy` — faster for pure text but duplicates data already in SQLite.
- Naive `LIKE` queries — too slow for large libraries.

**Rationale:** FTS5 is built into SQLite (compile-time flag in rusqlite). It provides relevance ranking, prefix queries, and phrase queries. One less dependency.

---

## ADR-013: Key bindings — Vim-inspired, TOML-configured, hot-reloadable

**Decision:** Key binding model: `Mode → KeySequence → Action`. Modes: Normal, Insert (search), Command, Visual.

**Alternatives considered:**
- Emacs-style — less familiar to target audience (cmus/vim users).
- Hardcoded — not configurable.

**Rationale:** Multi-modal key bindings with configurable key sequences. `Action` enum dispatched through event bus. Bindings stored in `bindings.toml`, hot-reloadable.

---

## ADR-014: Mouse support — first-class

**Decision:** Full mouse support integrated at the app level, not per-widget hacks.

**Rationale:** crossterm supports `EnableMouseCapture`, `MouseEventKind`, and position reporting. Each widget receives mouse events translated to its local coordinate space. Drag-and-drop, resize handles, scroll, and context menus are all first-class.

**Mouse event flow:**
```
crossterm MouseEvent → InputHandler → MouseEvent { position, kind, modifiers } → EventRouter
→ Screen finds widget at position → widget.handle_mouse_event(relative_pos, kind)
```

---

## ADR-015: Command system — colon-prefixed with completions

**Decision:** Command palette triggered by `:`. Commands implement `Command` trait. Completion engine provides tab-completion for command names and arguments.

**Rationale:** Vim-like command mode is discoverable and keyboard-friendly. Each command registers itself with the `CommandRegistry` at startup. The command parser splits `:command arg1 arg2` into `CommandCall { name, args }` and dispatches via event bus.

---

## ADR-016: Theme system — TOML-based with hot-swap

**Decision:** Themes defined as TOML files mapping semantic color names to RGB values. ratatui's `Style` constructed from theme on render. Theme change event triggers `mark_all_dirty()`.

**Rationale:** Separating theme from widget code enables community themes. Semantic names (e.g., `library.selected`, `statusbar.playing`) decouple theme from specific widget implementations.

---

## ADR-017: Testing strategy

| Level | Scope | Tool |
|-------|-------|------|
| Unit | Per-module logic, traits | `#[cfg(test)] mod tests` |
| Integration | Component interactions | `tests/` directory |
| Snapshot | UI rendering | `insta` (planned) |
| Fuzz | Input parsing | `proptest` (planned) |
| Bench | Audio pipeline, search | `criterion` (planned) |

**Rationale:** Unit tests validate individual components. Integration tests wire components together with test channels. UI snapshot tests prevent regressions in rendering output.

---

## ADR-018: Code organization — module-based, no workspaces initially

**Decision:** Single crate with `src/` module tree. Extract to workspace crates only when subsystems become large enough to warrant independent compilation.

**Rationale:** Workspaces add Cargo.toml overhead for every subcrate. Start monolithic but modular. Extraction path: `src/audio/ → crates/tanu-audio/`.

---

## ADR-019: Concurrency model — actors over shared state

**Decision:** Components are actors with owned state, communicating via channels. No `Arc<Mutex<T>>` for component state.

**Rationale:** Actor model prevents deadlocks and data races by design. Each component owns its data and processes events sequentially. Testable: send event, assert response event.

**Trade-off:** Slightly more boilerplate than shared state. Worth it for correctness.

---

## ADR-020: Rendering — differential with dirty tracking

**Decision:** Widgets set `dirty = true` on state change. `Screen` collects dirty widgets, sorts by layer, renders only dirty regions.

**Rationale:** Terminal rendering is expensive (write syscalls). Redrawing only changed regions keeps UI smooth with large libraries. ratatui's `Buffer` diff support enables efficient `Frame::render_widget` calls.

---

## Summary

| Decision | Choice |
|----------|--------|
| Runtime | tokio |
| TUI | ratatui + crossterm |
| Audio decode | symphonia |
| Audio output | rodio (trait-abstracted) |
| Metadata | lofty |
| Database | rusqlite + spawn_blocking |
| Search | SQLite FTS5 |
| Events | mpsc channels (actor model) |
| Config | serde + TOML, hot-reload |
| Plugins | Traits (v1), WASM (v2) |
| Errors | thiserror + anyhow |
| Key bindings | Vim-modal, TOML-configurable |
| Mouse | First-class, coordinate-aware |
| Themes | TOML with semantic names |
| Rendering | Differential, dirty-tracking |
| Testing | Unit + integration + snapshot |
| Code org | Single crate, modular |
| Concurrency | Actor model, message-passing |
