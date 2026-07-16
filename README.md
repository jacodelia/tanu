# Tanu

![Tanu](docs/tanu.jpg) photo by [Ebustos](https://estebanbustos.com/products/tanu?srsltid=AfmBOoozUW3g4BeDI7p6OIXNbmj2xMCP4DwPspE4_uUeaysFOH-MX1nU)

A terminal music player written in Rust — a file-browser–first player inspired
by [cmus](https://cmus.github.io/), with a [ratune](https://github.com/acmagn/ratune)-style
layout and a [ratatui-explorer](https://github.com/tatounee/ratatui-explorer)-style
file explorer.

Browse your filesystem, hit Enter to play, watch a real-time oscilloscope fed
from the actual audio, see the embedded album art, and drive everything from a
radio-cassette transport deck — keyboard or mouse.

```
┌ FILE  EDIT  ABOUT             ───────────────────────────| ♪ Tanu |──┐
│ 🗀 ~/Music                          │ ♫ Cover                        │
│ ⤴  ..                               │  ▄▄▄▄▄▄▄▄▄▄▄▄                  │
│ ▸  Albums                           │  ██ album art ██               │
│ ▶ ♪ track-01.flac                   ├────────────────────────────────┤
│   ♪ track-02.flac                   │ ≣ Equalizer · 12  ▄█▆▃▅█▇▂▄▆█▅ │
│   ...                               ├────────────────────────────────┤
│                                     │ Oscilloscope ▶  /\  /\  /\     │
├─────────────────────────────────────┴────────────────────────────────┤
│ ▚ TAPE DECK ▞                                                        │
│  ╔═══╗╔═══╗╔═══╗╔═══╗╔═══╗╔═══╗                                      │
│  ║ ◀◀║║ ▶ ║║ ■ ║║ ▶▶║║ ⇄ ║║ ↻ ║   00:42 ██████░░░░░ 03:15            │
│  ╚═══╝╚═══╝╚═══╝╚═══╝╚═══╝╚═══╝    VOL ▐▓▓▓▓░░░▌  80%                │
└──────────────────────────────────────────────────────────────────────┘
```

## Features

- **File explorer** — an expandable directory tree (dirs expand in place),
  media files only, icons, selection marker, incremental search, hidden-file
  toggle. Keyboard + mouse. Playing a file queues its folder in tree order so
  Prev/Next step through it.
- **Instant playback** — streaming decode via rodio; playback starts in
  microseconds (no full-file decode up front).
- **Real oscilloscope** — waveform drawn from the samples actually playing.
- **12-band equalizer** — Goertzel spectrum analyzer with smoothed, color-graded
  bars, between the cover and the scope.
- **Album art** — embedded cover rendered as a half-block mosaic, aspect-preserved
  and Lanczos3-scaled (no image protocol needed; works in any terminal).
- **Radio-cassette transport deck** — prev / play-pause / stop / next /
  shuffle / repeat, progress bar, and a horizontal volume bar. All clickable.
- **Menus** — FILE (open / library folder / scan / quit), EDIT (sound source),
  ABOUT.
- **Formats** — MP3, FLAC, OGG, Opus, WAV, M4A (via symphonia).
- **Responsive** — adapts from a 5" screen upward.

## Build & run

Requires a recent Rust toolchain and an audio output (ALSA on Linux).

```sh
cargo build --release
cargo run --release -- [MUSIC_DIR ...]
```

With no arguments, tanu opens your saved library folder (or your system audio
directory). Set a start folder from **FILE → Library Folder…**.

Logs go to `~/.local/share/tanu/tanu.log` (never stdout — the TUI owns the
terminal). Follow them with `tail -f ~/.local/share/tanu/tanu.log`.

## Keys

| Key | Action |
|-----|--------|
| `↑`/`k`, `↓`/`j` | Move selection |
| `→`/`l` | Expand directory / descend |
| `Enter` | Toggle directory / play file |
| `←`/`h`/`Backspace` | Collapse directory / go to parent |
| `◀◀` / `▶▶` (or Prev/Next) | Previous / next media file in the folder |
| `Space` | Play selected (if idle) / pause-resume |
| `+` / `=` / `-` | Volume up / down |
| `/` | Incremental search (Esc cancels, Enter keeps filter) |
| `Ctrl+H` | Toggle hidden files |
| `Home`/`g`, `End`/`G`, `PgUp`/`PgDn` | Jump / page |
| `:` | Command mode |
| `q`, `Ctrl+C`, `Ctrl+Q` | Quit |

Mouse: click to select, double-click to play/enter, right-click for a context
menu, scroll to navigate. Click the transport keys, the volume bar, and the
menu items.

## Commands (`:`)

`play` · `pause` · `stop` · `next` · `previous` · `toggle` · `volume <0-100>` ·
`seek <s>` · `shuffle on|off` · `repeat off|track|playlist` · `rescan` ·
`theme <name>|list` · `layout <name>|list` · `set <key> <value>` ·
`library_folder` · `open_file` · `scan_dir` · `about` · `quit`

`:set` persists to `config.toml` — keys: `theme`, `volume`, `library`, `max_fps`.

## Config

`~/.config/tanu/config.toml` (created on first `:set` / library-folder change).
Themes and key bindings live under the same directory.

## Development

```sh
cargo test              # unit + doctests
cargo build --all-targets
cargo bench             # criterion (db_benchmark)
```

Optional feature flags: `http-plugins` (reqwest), `wasm-plugins` (wasmtime).

See [`docs/roadmap.md`](docs/roadmap.md) for what's done and what's next, and
[`docs/adr.md`](docs/adr.md) for architecture decisions.

## License

[MIT](LICENSE)
