# tanu

![tanu](docs/tanu.jpg)

A terminal music player written in Rust — a file-browser–first player inspired
by [cmus](https://cmus.github.io/), with a [ratune](https://github.com/acmagn/ratune)-style
layout and a [ratatui-explorer](https://github.com/tatounee/ratatui-explorer)-style
file explorer.

Browse your filesystem, hit Enter to play, watch a real-time oscilloscope fed
from the actual audio, see the embedded album art, and drive everything from a
radio-cassette transport deck — keyboard or mouse.

```
┌ FILE  EDIT  ABOUT              ♪ tanu ──────────────────────────────┐
│ 🗀 ~/Music                          │ ♫ Cover                        │
│ ⤴  ..                               │  ▄▄▄▄▄▄▄▄▄▄▄▄                   │
│ ▸  Albums                           │  ██ album art ██               │
│ ▶ ♪ track-01.flac                   ├────────────────────────────────┤
│   ♪ track-02.flac                   │ Oscilloscope ▶  /\  /\  /\      │
│   ...                               │               /  \/  \/  \     │
├─────────────────────────────────────┴────────────────────────────────┤
│ ▚ TAPE DECK ▞                                                         │
│  ╔═══╗╔═══╗╔═══╗╔═══╗╔═══╗╔═══╗                                       │
│  ║ ◀◀║║ ▶ ║║ ■ ║║ ▶▶║║ ⇄ ║║ ↻ ║   00:42 ██████░░░░░ 03:15            │
│  ╚═══╝╚═══╝╚═══╝╚═══╝╚═══╝╚═══╝    VOL ▐▓▓▓▓░░░▌  80%                 │
└───────────────────────────────────────────────────────────────────────┘
```

## Features

- **File explorer** — `..` parent nav, directories first, icons, selection
  marker, incremental search, hidden-file toggle. Keyboard + mouse.
- **Instant playback** — streaming decode via rodio; playback starts in
  microseconds (no full-file decode up front).
- **Real oscilloscope** — waveform drawn from the samples actually playing.
- **Album art** — embedded cover rendered as half-block mosaic (no image
  protocol needed; works in any terminal).
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
| `→`/`l`/`Enter` | Enter directory / play file |
| `←`/`h`/`Backspace` | Parent directory |
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

See repository.
