//! Tanu — a modern terminal music player inspired by cmus.
//!
//! Public API re-exports for benchmarks and integration tests.

// ponytail: WIP — several subsystems (plugins, playlists, search, browser)
// are implemented but not yet wired into `app`. Remove once they're connected.
#![allow(dead_code)]

pub mod app;
pub mod audio;
pub mod browser;
pub mod commands;
pub mod config;
pub mod core;
pub mod database;
pub mod events;
pub mod input;
pub mod library;
pub mod mouse;
pub mod player;
pub mod playlist;
pub mod plugins;
pub mod queue;
pub mod search;
pub mod services;
pub mod theme;
pub mod ui;
pub mod widgets;
