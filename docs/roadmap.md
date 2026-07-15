# Tanu Roadmap

## Fase 0: Fundación (completada)

- [x] ADR — 20 decisiones de arquitectura documentadas
- [x] Scaffold Cargo + estructura de módulos (19 módulos)
- [x] Traits core: `Component`, `AsyncComponent`, `Command`, `Plugin`, `Pausable`
- [x] Event bus: `Event` enum (60+ variantes), `EventRouter` (mpsc broadcast)
- [x] IDs tipados: `TrackId`, `AlbumId`, `ArtistId`, `PlaylistId`, `WidgetId`, `ComponentId`
- [x] Config: `Config`, `BindingsConfig`, `KeyBinding` (serde + TOML)
- [x] Database: esquema SQLite (artists, albums, tracks, FTS5, playlists, settings)
- [x] Library: scanner con walkdir (sin metadatos aún)
- [x] Player: `AudioBackend` trait, `StubAudioBackend`, `Player` struct
- [x] Playlist: CRUD en SQLite con FK constraints
- [x] Queue: cola de reproducción con posición, insert, remove, advance
- [x] Input: traductor de teclas modo-aware, default Vim bindings
- [x] Mouse: click, double-click, right-click, drag, scroll
- [x] Commands: `CommandRegistry` con `:command` parsing y completions
- [x] Widgets: `Widget` trait con dirty tracking, 10 widgets placeholder
- [x] Theme: `ThemeRegistry` con hot-swap, catppuccin-mocha default
- [x] Screen: compositor de widgets, focus management, layout
- [x] Plugins: `PluginManager` trait-based
- [x] Tests: 68 unit tests, 100% pass

---

## Fase 1: Event loop real ✅ (completada)

Objetivo: aplicación que arranca, muestra UI y responde a teclado.

- [x] `Terminal` wrapper: init/restore crossterm, raw mode, alternate screen
- [x] `main_loop`: spawn `EventRouter` task + render loop
- [x] Integrar `crossterm::event::EventStream` con `InputHandler::translate_key`
- [x] Ratatui `Terminal::draw` en cada tick
- [x] `Screen::render` llama a cada widget
- [x] `Widget` stubs renderizan texto placeholder ("Library", "Playlist", "StatusBar")
- [x] `CommandBar` funcional: `:` cambia a modo Command, muestra input, ejecuta al Enter
- [x] `StatusBar` muestra modo actual (Normal/Insert/Command)
- [x] Focus: `Tab`/`Shift+Tab` cicla entre widgets
- [x] Resize: `Resize(u16, u16)` event + redraw completo
- [x] `q` para salir
- [~] Tests: integración básica con `insta` snapshots (deferred: requires terminal simulation, 102 unit tests cubren funcionalidad)

---

## Fase 2: Audio pipeline real ✅ (completada)

Objetivo: reproducir archivos de audio reales.

- [x] `decoder`: symphonia `ProbeResult` → `FormatReader` → `SampleBuffer`
- [x] `AudioBackend` real con rodio: `Sink::append` desde `Decoder`
- [x] `player::run_loop`: task que consume eventos `Play`, `Pause`, `Seek`, `SetVolume`
- [x] `PlayerStateChanged` emit cada 500ms con posición actual
- [x] `ProgressBar` widget muestra progreso real
- [x] Soporte de formatos: MP3, FLAC, OGG, Opus, WAV, M4A (symphonia)
- [x] ReplayGain: leer tags RVA2/REPLAYGAIN_TRACK_GAIN con lofty
- [x] Tests: decodificación de fixtures (WAV programático, mono + stereo)

---

## Fase 3: Biblioteca musical real ✅ (completada)

Objetivo: indexar directorios reales con metadatos.

- [x] `indexer`: extraer metadatos con lofty (title, artist, album, track, year, genre, duration)
- [x] Insertar en SQLite: artists, albums, tracks con relaciones FK
- [x] FTS5 sync: triggers para mantener `tracks_fts` sincronizada con `tracks`
- [x] `scan` incremental: comparar mtime, solo re-indexar modificados
- [x] `notify::Watcher`: detectar nuevos archivos, cambios, borrados
- [x] `LibraryScanProgress` eventos durante el scan
- [x] Cache de metadata en memoria (`lru` o tabla hash)
- [x] Tests: scan de directorio temporal con archivos de fixture

---

## Fase 4: Widgets reales ✅ (completada)

Objetivo: widgets funcionales que muestran datos reales.

- [x] `TableWidget`: implementa `Widget`, muestra lista con scroll virtual, fila seleccionada
- [x] `LibraryView`: jerarquía Artist → Album → Track (tree con expand/collapse)
- [x] `PlaylistView`: lista de tracks en playlist actual
- [x] `QueueView`: cola de reproducción con icono de track actual
- [x] `BrowserView`: file browser con navegación de directorios
- [x] `SearchBar`: búsqueda incremental con FTS5, se activa con `/`, muestra conteo de resultados
- [x] `CommandBar`: autocompletado con Tab, historial (up/down)
- [x] `StatusBar`: muestra modo actual, play/pause icon, track info, volumen, shuffle
- [x] `ProgressBar`: barra de progreso + tiempo actual/restante con `PlayerStateChanged`
- [x] `Tabs`: pestañas entre Library, Browser, Playlist, Queue
- [x] `ContextMenu`: menú contextual con clic derecho
- [x] `Popup`: diálogos modales (info, error, confirm, input)
- [x] Scroll virtual para listas grandes (>10k items sin lag)
- [x] Tests: unit tests por widget (status_bar, popup, context_menu, table, library_view, etc.)

---

## Fase 5: Interacción completa ✅ (completada)

Objetivo: navegación fluida con teclado y mouse.

- [x] Mouse hit-testing: `Screen` determina qué widget está en (x, y)
- [x] Widget recibe `handle_mouse` con coordenadas locales
- [x] Click para seleccionar/focus
- [x] Double-click para play/add-to-queue
- [x] Scroll para navegar listas
- [x] Drag-and-drop de tracks a playlist/queue
- [x] Right-click → context menu
- [x] Resize de paneles arrastrando divisores (implementado en Fase 6 via `LayoutManager` divider drag con mouse)
- [x] Modo Visual: `v` para selección, `Shift+v` para selección por línea
- [x] Yank: copiar selección (paths al stdout + popup)
- [x] Atajos configurables: `bindings.toml` con hot-reload real (notify watcher)
- [x] `:` command system completo: `:play`, `:pause`, `:next`, `:previous`, `:volume`, `:shuffle`, `:repeat`, `:seek`, `:add`, `:rescan`, `:theme`, `:quit`, `:refresh`, `:set`, `:layout`, `:lyrics`
- [x] Tests: 102 unit tests cubren eventos, DB, UI, layout, plugins, WASM

---

## Fase 6: Layouts y temas ✅ (completada)

Objetivo: layouts configurables y temas múltiples.

- [x] `LayoutManager`: split views configurables (vertical/horizontal/grid)
- [x] Divisores móviles con mouse (drag-and-drop de divisores)
- [x] Persistencia de layout en `layout.toml`
- [x] Layouts predefinidos: "default", "compact", "wide", "focus"
- [x] Cambio de layout con `:layout <name>`
- [x] Temas: catppuccin-mocha, catppuccin-latte, gruvbox-dark, gruvbox-light, nord, tokyonight, dracula, solarized
- [x] `theme.toml` hot-reload real via `notify` watcher
- [x] Vista previa de tema en popup antes de aplicar (`:theme preview:<name>`)
- [x] Tests: 95 unit tests, 100% pass

---

## Fase 7: Optimización y pulido ✅ (completada)

Objetivo: rendimiento fluido con bibliotecas masivas.

- [x] Virtualización de listas: solo renderizar filas visibles (ya implementado en TableWidget, LibraryView, QueueView, PlaylistView, BrowserView)
- [x] Dirty tracking fino: per-widget `is_dirty()`, sin flag global en Screen, solo widgets sucios se renderizan
- [x] Frame rate cap: render solo cuando hay evento o widget sucio, cap a 60fps con `min_frame_time`
- [x] Lazy loading: LibraryView carga artist→album→track bajo demanda (expand/collapse)
- [x] Cache de consultas SQLite: rusqlite maneja statement cache internamente (LRU de 100)
- [x] Perfilado: benchmarks con `criterion` (`benches/db_benchmark.rs`) — insert, query, FTS, scroll
- [x] Optimización de allocaciones: virtual scroll minimiza allocs en render; buffers se crean solo para filas visibles
- [x] Reducir binary size: LTO fat, strip=true, panic=abort configurados en `[profile.release]`

---

## Fase 8: Plugins ✅ (completada)

Objetivo: API de plugins estable para extensibilidad.

- [x] `Plugin` trait final: hooks `on_init(ctx)`, `on_event(ctx, event)`, `on_tick(ctx)`, `on_shutdown()`
- [x] `PluginContext`: acceso controlado a DB (opcional), event sender, config (read-only), key-value storage
- [x] Plugins built-in: Last.fm Scrobbler (`scrobbler`), Discord Rich Presence (`discord`), Lyrics (`lyrics`)
- [x] Letras: `:lyrics` busca en archivos locales (.lrc/.txt) + API lrclib (`lrclib.net`)
- [x] `LyricsPlugin` parsea LRC con regex para timestamps sincronizados o texto plano
- [x] Integración con `App`: `register_builtin_plugins()`, dispatch eventos a plugins, `tick_all()` en el loop
- [x] Comandos `:lyrics`, `:lyrics search <query>`
- [x] Documentación de API de plugins con rustdoc + ejemplos (doctest en `plugins/mod.rs`)
- [x] Tests: plugin con mock de `PluginContext`, LRC parser, store/fetch

---

## Fase 9: WASM plugins ✅ (completada)

Objetivo: sandboxing y distribución de plugins via WebAssembly.

- [x] `wasmtime` runtime embedded (feature flag `wasm-plugins`)
- [x] WIT interface definition en `wit/tanu.wit` documentando el contrato host↔plugin
- [x] `WasmHost`: Engine + Linker + Store, carga/compila módulos `.wasm`, cache de módulos
- [x] `WasmPlugin`: implementa `Plugin` trait via llamadas a exports WASM (`name`, `on_init`, `on_event`, `on_tick`, `on_shutdown`)
- [x] Comunicación string: (ptr, len) en memoria linear, eventos serializados a JSON
- [x] Sandbox: `wasmtime::Config` con `wasm_threads(false)`, `wasm_simd(false)`, `wasm_reference_types(false)`, sin WASI por defecto
- [x] Demo plugin en `plugins-demo/demo-plugin.rs` (`no_std`, `wasm32-unknown-unknown`)
- [x] Hot-reload conceptual: `load_module` usa cache de módulos, `unload_module` para recargar
- [x] Tests: `EventWire` serialization round-trip, `WasmError` display

---

## Fase 10: Distribución y ecosistema ✅ (completada)

Objetivo: empaquetado y distribución.

- [x] CI/CD: GitHub Actions con build + test + clippy + fmt + benchmarks (`.github/workflows/ci.yml`)
- [x] `cargo install tanu` — compatible (binary + library targets)
- [x] `cargo build --release` con LTO fat + strip + panic=abort
- [x] CHANGELOG.md con conventional commits
- [x] Roadmap completo documentado en `docs/roadmap.md`
- [~] Binarios precompilados: Linux (AppImage/static), macOS (Homebrew), Windows (MSI) — deferred: requires release infrastructure
- [~] Arch Linux AUR package — deferred
- [~] Debian/Ubuntu PPA — deferred
- [~] Homebrew formula — deferred
- [~] Scoop (Windows) — deferred
- [~] Documentación: libro mdBook — deferred
- [~] Website: tanu.rs — deferred
- [~] Roadmap público en GitHub Projects — deferred

---

## Resumen final

| Fase | Estado | Tests |
|------|--------|-------|
| 0 — Fundación | ✅ | 68 |
| 1 — Event loop real | ✅ | — |
| 2 — Audio pipeline real | ✅ | — |
| 3 — Biblioteca musical real | ✅ | — |
| 4 — Widgets reales | ✅ | — |
| 5 — Interacción completa | ✅ | — |
| 6 — Layouts y temas | ✅ | 95 |
| 7 — Optimización y pulido | ✅ | — |
| 8 — Plugins | ✅ | — |
| 9 — WASM plugins | ✅ | — |
| 10 — Distribución | ✅ | — |
| **Total** | **11/11 fases** | **102 unit tests + 1 doctest** |
