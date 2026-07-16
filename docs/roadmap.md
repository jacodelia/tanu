# Tanu Roadmap

Tanu es un reproductor de música de terminal en Rust, inspirado en cmus y en
el layout de [ratune](https://github.com/acmagn/ratune), con un explorador de
archivos al estilo de [ratatui-explorer](https://github.com/tatounee/ratatui-explorer).

Este roadmap refleja el estado **real** del código (verificado contra la
compilación y los tests), no un plan aspiracional.

---

## ✅ Hecho

### Núcleo y arranque
- [x] Event bus (`Event`, `EventRouter` mpsc/broadcast), IDs tipados, config TOML.
- [x] Base de datos SQLite (artists/albums/tracks/FTS5/playlists/settings).
- [x] Escáner de biblioteca con `lofty` + `walkdir`, scan incremental por mtime.
- [x] Pipeline de audio real con `rodio` + `symphonia` (MP3/FLAC/OGG/Opus/WAV/M4A).
- [x] Logging a archivo (`~/.local/share/tanu/tanu.log`) — nunca a stdout, para no
      corromper la TUI.

### UI — layout estilo ratune
- [x] Stack vertical: **menú → explorador (principal) → osciloscopio →
      transport deck → status bar**.
- [x] Layout responsivo: en pantallas pequeñas (≈5") se ocultan osciloscopio,
      command bar; los paneles nunca colapsan a altura cero (`Constraint::Fill`).
- [x] Render correcto: todos los widgets se dibujan cada frame (arreglado el bug
      de "las letras desaparecen al presionar teclas").

### Explorador de archivos (estilo ratatui-explorer)
- [x] Vista de un directorio con entrada `..`, directorios primero, orden alfabético.
- [x] Iconos por tipo (`▸` dir, `♪` archivo, `⤴` padre) + marcador de selección `▶`.
- [x] Ruta actual en el título; contador `n/total` y flag de ocultos abajo.
- [x] Navegación teclado: `↑/k` `↓/j`, `←/h/Backspace` (subir), `→/l/Enter` (entrar/reproducir),
      `Home/g` `End/G`, `PageUp/PageDown`, `Ctrl+H` (ocultos on/off).
- [x] Navegación mouse: click selecciona, doble-click abre/reproduce, scroll.
- [x] Al subir de directorio, la selección aterriza en la carpeta de la que se salió.

### Reproducción
- [x] Enter / doble-click reproduce el archivo (`play_file:` → `PlayPath` → player).
- [x] **Arranque instantáneo**: decodificación en streaming vía rodio `Decoder`
      (play() retorna en ~0.5ms; duración leída rápido con lofty). Sin el delay
      del decode completo previo.
- [x] **Espacio inteligente**: reproduce el archivo seleccionado si nada suena,
      pausa/reanuda si hay reproducción.
- [x] Los eventos `PlayerStateChanged` ahora llegan a los widgets (router → UI):
      la barra de progreso y los tiempos avanzan de verdad.
- [x] Ctrl+C / Ctrl+Q: salir (en cualquier modo). `q` también sale.

### Control de volumen
- [x] Barra horizontal en el deck: `+`/`=` sube, `-` baja; click en la barra fija el nivel.

### Album art
- [x] Cuadro a la derecha con la carátula embebida del track (lofty + `image`),
      renderizada con medios-bloques `▀` (sin protocolos de imagen). Placeholder ♪ si no hay.

### Transport deck (estilo radio-cassette)
- [x] Panel con teclas gruesas: `◀◀` prev, `▶`/`‖` play-pausa, `■` stop, `▶▶` next,
      `⇄` shuffle, `↻` repeat (Off→Track→Playlist), + barra de volumen.
- [x] Barra de progreso con posición/duración; estados activos resaltados.
- [x] Botones y barra de volumen clicables con el mouse (arreglado: las regiones
      de hit-test estaban en coords absolutas en vez de locales al widget).

### Osciloscopio (waveform real)
- [x] Visualizador de onda **real**: lee una ventana de muestras en el playhead
      desde un buffer compartido (`AudioViz`), no una onda sintetizada.
- [x] El backend guarda una copia diezmada mono al decodificar; el widget dibuja
      la forma de onda del audio que suena (Canvas braille), ~25fps.
- [x] Línea plana cuando no hay reproducción; descansa cuando no se ve.

### Reproducción encadenada
- [x] Al terminar un track avanza automáticamente según el modo repeat:
      Track (repite), Playlist (siguiente/vuelve al inicio), Off (siguiente o para).

### Búsqueda incremental en el explorador
- [x] `/` inicia búsqueda; se filtra el directorio en vivo por nombre (case-insensitive).
      Enter fija el filtro, Esc lo cancela. El filtro se muestra en el borde inferior.

### Persistencia de config (`:set`)
- [x] `:set theme|volume|library|max_fps <valor>` aplica y guarda en `config.toml`.

### Menús (barra superior)
- [x] `FILE`: Open File… · Library Folder… · Scan Directory… · Quit.
- [x] `EDIT`: Sound Source…  ·  `ABOUT`: versión + ayuda.
- [x] Menús desplegables y sus items son clicables con el mouse.
- [x] **Library Folder** persiste el directorio de inicio en `config.toml` y apunta
      el explorador allí (única función de la "biblioteca": saber desde dónde iniciar).

### Mouse
- [x] Pipeline vía `MouseHandler`: click, doble-click, click-derecho (menú contextual),
      scroll, drag de divisores (en layouts que los tienen).

---

## 🔜 Próximo

- [ ] **Cola de reproducción visible**: reintroducir una vista de Queue/Now-Playing
      navegable (hoy el player tiene cola interna pero sin UI).
- [ ] **Selección de dispositivo de salida** real (EDIT → Sound Source hoy solo registra;
      rodio requiere reconstruir el stream por dispositivo, con cuidado por `!Send`).
- [ ] **Playlists**: UI para crear/editar/guardar (CRUD en DB ya existe).
- [ ] **Shuffle real**: `check_track_end` avanza secuencial; falta orden aleatorio.
- [ ] **FFT / espectro** como alternativa al osciloscopio (estilo ratune).
- [ ] **Seek**: rodio no soporta seek nativo; requiere re-decodificar desde la posición.

---

## 🧊 Backlog / diferido

- [ ] Plugins WASM (`wasmtime`) — código presente, sin activar por defecto.
- [ ] Plugins built-in (scrobbler, discord, lyrics) — registrados pero requieren
      credenciales/red para ser útiles.
- [ ] Temas múltiples y hot-reload (infra presente).
- [ ] Binarios precompilados / empaquetado (AUR, Homebrew, Scoop, AppImage).
- [ ] Documentación mdBook + website.

---

## Estado

| Área | Estado |
|------|--------|
| Explorador de archivos | ✅ estilo ratatui-explorer |
| Layout | ✅ estilo ratune (stack vertical, responsivo) |
| Reproducción básica + encadenada | ✅ (enter/espacio/transport/auto-advance) |
| Osciloscopio real (waveform en playhead) | ✅ |
| Búsqueda incremental (`/`) | ✅ |
| Persistencia `:set` | ✅ |
| Menús + mouse | ✅ |
| Cola/Playlists UI | 🔜 pendiente |
| Selección de dispositivo de salida | 🔜 pendiente |

Tests: `cargo test` — 109 unit + 1 doctest, 100% pass. `cargo build --all-targets`: 0 warnings.
