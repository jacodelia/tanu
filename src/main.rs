// ponytail: WIP — subsystems implemented but not yet wired into `app`.
// Remove once they're connected.
#![allow(dead_code)]

mod app;
mod audio;
mod browser;
mod commands;
mod config;
mod core;
mod database;
mod events;
mod input;
mod library;
mod mouse;
mod player;
mod playlist;
mod plugins;
mod queue;
mod search;
mod services;
mod theme;
mod ui;
mod widgets;

use std::panic;
use std::path::PathBuf;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to a file, never stdout — the TUI owns the terminal.
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tanu");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file = tracing_appender::rolling::never(&log_dir, "tanu.log");
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .with_ansi(false)
        .with_writer(log_file)
        .init();

    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = app::terminal::Terminal::teardown();
        default_hook(info);
    }));

    let mut app = app::App::default_app();
    app.register_builtin_plugins();

    // Collect music paths from CLI args, or use defaults
    let music_paths: Vec<PathBuf> = {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.is_empty() {
            // Default to ~/Music
            vec![dirs::audio_dir().unwrap_or_else(|| PathBuf::from("."))]
        } else {
            args.into_iter().map(PathBuf::from).collect()
        }
    };

    // Spawn the audio player
    app.spawn_player(music_paths.clone())?;

    // Start library scanner in background
    let db_path = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tanu")
        .join("tanu.db");
    if let Ok(db) = database::Database::open(&db_path) {
        app.scan_library(db, music_paths);
    }

    // Take the router and spawn it as a background task
    let mut router = app.take_router();
    tokio::spawn(async move {
        router.run().await;
    });

    // Initialize terminal and run the blocking TUI loop
    let mut terminal = app::terminal::Terminal::new()?;
    app::terminal::Terminal::setup()?;

    let result = app.run(&mut terminal);

    let _ = app.sender().send(events::Event::Quit);
    let _ = app::terminal::Terminal::teardown();

    result
}
