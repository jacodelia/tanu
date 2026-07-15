# Changelog

All notable changes to Tanu will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] — Unreleased

### Added
- **Core**: Modular architecture with 21 source modules
- **TUI**: Ratatui + crossterm terminal UI with Vim-inspired keybindings
- **Audio**: Symphonia decoder + Rodio output backend (MP3, FLAC, OGG, Opus, WAV, M4A)
- **Library**: SQLite-backed music library with FTS5 full-text search
- **Widgets**: 12 interactive widgets (tabs, search, library tree, playlist, queue, browser, progress, status, command bar, popup, context menu, table)
- **Commands**: `:play`, `:pause`, `:next`, `:previous`, `:volume`, `:shuffle`, `:repeat`, `:seek`, `:add`, `:rescan`, `:theme`, `:layout`, `:lyrics`, `:quit`, `:refresh`, `:set`
- **Themes**: 8 built-in themes (catppuccin-mocha/latte, gruvbox-dark/light, nord, tokyonight, dracula, solarized)
- **Layouts**: 4 predefined layouts (default, compact, wide, focus) with movable dividers
- **Mouse**: Full mouse support (click, double-click, right-click, scroll, drag-and-drop)
- **Plugins**: Trait-based plugin API with PluginContext, built-in Last.fm scrobbler, Discord Rich Presence, and lyrics plugins
- **WASM**: Experimental WASM plugin runtime via wasmtime (feature-gated)
- **CI/CD**: GitHub Actions CI (build, test, clippy, fmt, benchmark check)
- **Performance**: Virtual scroll, per-widget dirty tracking, frame rate cap, SQLite caching
