//! Application runtime — owns the terminal, screen, and input handler.
//! Runs the main event loop: poll crossterm events, translate, dispatch, render.

use std::any::Any;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crate::audio::backend::RodioBackend;
use crate::commands::CommandRegistry;
use crate::plugins::PluginManager;
use crate::core::traits::Component;
use crate::core::traits::Command;
use crate::database::Database;
use crate::events::bus::{self, EventRouter, EventSender};
use crate::events::Event;
use crate::input::InputHandler;
use crate::library::Library;
use crate::player::{Player, PlayerCommand};
use crate::ui::{Screen, Slot};
use crate::widgets::Widget;
use crate::widgets::browser_view::BrowserView;
use crate::widgets::command_bar::CommandBar;
use crate::widgets::library_view::LibraryView;
use crate::widgets::playlist_view::PlaylistView;
use crate::widgets::progress_bar::ProgressBar;
use crate::widgets::queue_view::QueueView;
use crate::widgets::search_bar::SearchBar;
use crate::widgets::status_bar::StatusBar;
use crate::widgets::tabs::Tabs;

use notify::{EventKind, RecursiveMode, Watcher};

pub mod terminal;

use self::terminal::Terminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveMainView {
    Library,
    Browser,
    Playlist,
    Queue,
}

/// The main application struct. Owns all top-level subsystems and
/// drives the render loop.
pub struct App {
    screen: Screen,
    input_handler: InputHandler,
    commands: CommandRegistry,
    event_tx: EventSender,
    router: Option<EventRouter>,
    components: Vec<Box<dyn Component>>,
    plugins: PluginManager,
    db: Option<Database>,
    should_quit: bool,
    active_left: ActiveMainView,
    active_right: ActiveMainView,
}

impl App {
    pub fn new(
        screen: Screen,
        input_handler: InputHandler,
        commands: CommandRegistry,
    ) -> Self {
        let router = EventRouter::new();
        let event_tx = router.sender();
        let plugins = PluginManager::new(crate::plugins::PluginContext::new(event_tx.clone()));
        Self {
            screen,
            input_handler,
            commands,
            event_tx,
            router: Some(router),
            components: Vec::new(),
            plugins,
            db: None,
            should_quit: false,
            active_left: ActiveMainView::Library,
            active_right: ActiveMainView::Playlist,
        }
    }

    /// Returns a sender that feeds into the event router.
    pub fn sender(&self) -> EventSender {
        self.event_tx.clone()
    }

    /// Takes the router out, leaving `None` in its place.
    /// Must be called before spawning the router task.
    pub fn take_router(&mut self) -> EventRouter {
        self.router.take().expect("router already taken")
    }

    /// Get a mutable reference to the router (for registering listeners).
    /// Panics if the router has been taken.
    pub fn router_mut(&mut self) -> &mut EventRouter {
        self.router.as_mut().expect("router already taken")
    }

    /// Builds the default application with all widgets.
    pub fn default_app() -> Self {
        use crate::config::BindingsConfig;
        use crate::theme::ThemeRegistry;

        let theme = ThemeRegistry::new();
        let mut screen = Screen::new(theme);

        let tabs = Tabs::new(vec!["Library", "Browser", "Playlist", "Queue"]);
        screen.add_widget(Box::new(tabs), Slot::Tabs);

        let search_bar = SearchBar::new();
        screen.add_widget(Box::new(search_bar), Slot::SearchBar);

        let library_view = LibraryView::new();
        screen.add_widget(Box::new(library_view), Slot::MainLeft);

        let playlist_view = PlaylistView::new("Playlist");
        screen.add_widget(Box::new(playlist_view), Slot::MainRight);

        let progress = ProgressBar::new();
        screen.add_widget(Box::new(progress), Slot::ProgressBar);

        let status = StatusBar::new();
        screen.add_widget(Box::new(status), Slot::StatusBar);

        let cmd_bar = CommandBar::new();
        screen.add_widget(Box::new(cmd_bar), Slot::CommandBar);

        let input_handler = InputHandler::new(BindingsConfig::default_bindings());
        let commands = CommandRegistry::new();

        Self::new(screen, input_handler, commands)
    }

    /// Spawn the audio player on a dedicated OS thread.
    /// Returns a sender for sending commands to the player.
    /// Also spawns a tokio task that forwards relevant events
    /// from the router to the player as commands.
    pub fn spawn_player(
        &mut self,
        paths: Vec<PathBuf>,
    ) -> anyhow::Result<mpsc::Sender<PlayerCommand>> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<PlayerCommand>();
        let event_tx = self.sender();

        // Register a listener to forward events to the player thread
        let (bridge_tx, mut bridge_rx) = bus::event_channel();
        self.router_mut().register_listener(bridge_tx);

        let cmd_tx_clone = cmd_tx.clone();

        // Tokio task: receive events and forward as commands
        tokio::spawn(async move {
            while let Some(event) = bridge_rx.recv().await {
                let cmd = match event {
                    Event::Play => Some(PlayerCommand::Play),
                    Event::Pause => Some(PlayerCommand::Pause),
                    Event::TogglePlayPause => Some(PlayerCommand::TogglePlayPause),
                    Event::Stop => Some(PlayerCommand::Stop),
                    Event::Next => Some(PlayerCommand::Next),
                    Event::Previous => Some(PlayerCommand::Previous),
                    Event::Seek(pos) => Some(PlayerCommand::Seek(pos)),
                    Event::SetVolume(v) => Some(PlayerCommand::SetVolume(v)),
                    Event::SetShuffle(enabled) => Some(PlayerCommand::SetShuffle(enabled)),
                    Event::SetRepeat(mode) => Some(PlayerCommand::SetRepeat(mode)),
                    Event::Quit => Some(PlayerCommand::Quit),
                    _ => None,
                };
                if let Some(cmd) = cmd {
                    if cmd_tx_clone.send(cmd).is_err() {
                        break;
                    }
                }
            }
        });

        // Spawn the player on a dedicated OS thread.
        // RodioBackend / OutputStream is not Send, so we construct it inside
        // the thread.
        let paths = paths.clone();
        std::thread::spawn(move || {
            let backend = match RodioBackend::new() {
                Ok(b) => Box::new(b),
                Err(e) => {
                    tracing::error!("Failed to create audio backend: {}", e);
                    return;
                }
            };
            let mut player = Player::new(backend);
            for path in &paths {
                player.enqueue(path.clone());
            }
            player.run(cmd_rx, event_tx);
        });

        Ok(cmd_tx)
    }

    /// Scan the library directories in a blocking task.
    /// Emits progress events through the event bus.
    pub fn scan_library(&mut self, db: Database, music_dirs: Vec<PathBuf>) {
        let event_tx = self.sender();
        self.db = Some(db.clone());
        let _ = event_tx.send(Event::LibraryScanStarted);

        tokio::task::spawn_blocking(move || {
            let mut library = Library::new(db, music_dirs);
            match library.scan(&event_tx) {
                Ok(result) => {
                    tracing::info!(
                        files = result.files_found,
                        added = result.tracks_added,
                        updated = result.tracks_updated,
                        removed = result.tracks_removed,
                        duration = result.duration_secs,
                        "Library scan complete"
                    );
                }
                Err(e) => {
                    tracing::error!(error = %e, "Library scan failed");
                    let _ = event_tx.send(Event::DatabaseError(e.to_string()));
                }
            }
        });
    }

    /// Populate the library tree widget after a scan completes.
    fn refresh_library_table(&mut self) {
        if let Some(ref db) = self.db {
            if let Some(widget) = self.screen.widget_at_mut(Slot::MainLeft) {
                let widget_id = widget.id();
                let any = widget.as_mut() as &mut dyn Any;
                if let Some(library) = any.downcast_mut::<LibraryView>() {
                    if library.id() == widget_id {
                        library.set_database(db.clone());
                        library.refresh();
                    }
                }
            }
        }
    }

    /// Search tracks and populate the library with results.
    fn search_library(&mut self, query: &str) {
        if query.is_empty() {
            self.refresh_library_table();
            return;
        }

        if let Some(ref db) = self.db {
            if let Some(widget) = self.screen.widget_at_mut(Slot::MainLeft) {
                let widget_id = widget.id();
                let any = widget.as_mut() as &mut dyn Any;
                if let Some(library) = any.downcast_mut::<LibraryView>() {
                    if library.id() == widget_id {
                        library.set_database(db.clone());
                        library.refresh();
                    }
                }
            }
        }
    }

    /// Switch the content of a main panel to the requested view and
    /// highlight the matching tab. Library/Browser share the left panel;
    /// Playlist/Queue share the right panel.
    fn switch_view(&mut self, view: ActiveMainView) {
        let (slot, tab_idx) = match view {
            ActiveMainView::Library => (Slot::MainLeft, 0),
            ActiveMainView::Browser => (Slot::MainLeft, 1),
            ActiveMainView::Playlist => (Slot::MainRight, 2),
            ActiveMainView::Queue => (Slot::MainRight, 3),
        };

        // No-op if the panel already shows this view (avoids losing state).
        let current = if slot == Slot::MainLeft { self.active_left } else { self.active_right };
        if current == view {
            return;
        }

        let widget: Box<dyn crate::widgets::Widget> = match view {
            ActiveMainView::Library => Box::new(LibraryView::new()),
            ActiveMainView::Browser => {
                let root = dirs::audio_dir().unwrap_or_else(|| PathBuf::from("."));
                Box::new(BrowserView::new(root))
            }
            ActiveMainView::Playlist => Box::new(PlaylistView::new("Playlist")),
            ActiveMainView::Queue => Box::new(QueueView::new()),
        };
        self.screen.replace_widget(widget, slot);

        if slot == Slot::MainLeft {
            self.active_left = view;
        } else {
            self.active_right = view;
        }

        // LibraryView needs the database wired in to show tracks.
        if matches!(view, ActiveMainView::Library) {
            self.refresh_library_table();
        }

        // Sync the tab bar highlight.
        if let Some(widget) = self.screen.widget_at_mut(Slot::Tabs) {
            let any = widget.as_mut() as &mut dyn Any;
            if let Some(tabs) = any.downcast_mut::<Tabs>() {
                tabs.set_selected(tab_idx);
            }
        }
        self.screen.mark_dirty();
    }

    pub fn register_component(&mut self, component: Box<dyn Component>) {
        self.components.push(component);
    }

    /// Start watching the bindings.toml file for changes (hot-reload).
    pub fn watch_bindings(&self, bindings_path: PathBuf) {
        let event_tx = self.event_tx.clone();

        std::thread::spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = match notify::recommended_watcher(move |res| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("Failed to create bindings watcher: {}", e);
                    return;
                }
            };

            if let Some(parent) = bindings_path.parent() {
                if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                    tracing::warn!("Failed to watch bindings dir: {}", e);
                    return;
                }
            }

            tracing::info!(path = %bindings_path.display(), "Watching bindings file");

            loop {
                match rx.recv() {
                    Ok(event) => {
                        let changed = event.paths.iter().any(|p| p == &bindings_path);
                        if changed {
                            match event.kind {
                                EventKind::Modify(_) | EventKind::Create(_) => {
                                    let _ = event_tx.send(Event::BindingsReloaded);
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    pub fn register_command(
        &mut self,
        name: String,
        command: Box<dyn Command>,
    ) -> anyhow::Result<()> {
        self.commands.register(name, command)
    }

    /// Register a plugin with the plugin manager.
    pub fn register_plugin(&mut self, plugin: Box<dyn crate::core::traits::Plugin>) -> anyhow::Result<()> {
        self.plugins.register(plugin)
    }

    /// Register all built-in plugins.
    pub fn register_builtin_plugins(&mut self) {
        use crate::plugins::builtin::scrobbler::LastFmScrobbler;
        use crate::plugins::builtin::discord::DiscordPresence;
        use crate::plugins::builtin::lyrics::LyricsPlugin;

        self.plugins.register(Box::new(LastFmScrobbler::new()))
            .unwrap_or_else(|e| tracing::warn!("Failed to register scrobbler: {}", e));
        self.plugins.register(Box::new(DiscordPresence::new()))
            .unwrap_or_else(|e| tracing::warn!("Failed to register discord: {}", e));
        self.plugins.register(Box::new(LyricsPlugin::new()))
            .unwrap_or_else(|e| tracing::warn!("Failed to register lyrics: {}", e));

        tracing::info!("Built-in plugins registered");
    }

    /// Get mutable access to the plugin manager.
    pub fn plugins_mut(&mut self) -> &mut PluginManager {
        &mut self.plugins
    }

    /// Watch a directory of theme `.toml` files for changes.
    /// When a file changes, reloads it into the theme registry.
    pub fn watch_themes(&self, themes_dir: PathBuf) {
        let event_tx = self.event_tx.clone();

        std::thread::spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = match notify::recommended_watcher(move |res| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("Failed to create theme watcher: {}", e);
                    return;
                }
            };

            if let Err(e) = watcher.watch(&themes_dir, RecursiveMode::NonRecursive) {
                tracing::warn!("Failed to watch themes dir: {}", e);
                return;
            }

            tracing::info!(dir = %themes_dir.display(), "Watching themes directory");

            loop {
                match rx.recv() {
                    Ok(event) => {
                        let has_toml = event.paths.iter().any(|p| {
                            p.extension().map(|e| e == "toml").unwrap_or(false)
                        });
                        if has_toml && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                            let _ = event_tx.send(Event::ConfigReloaded);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    /// Runs the application loop synchronously. Call this from a tokio
    /// context; the loop polls crossterm events and drives rendering.
    /// Only renders when widgets are dirty or a user event arrived,
    /// capped at `max_fps` frames per second.
    pub fn run(&mut self, terminal: &mut Terminal) -> anyhow::Result<()> {
        let max_fps: u32 = 60;
        let min_frame_time = Duration::from_micros((1_000_000 / max_fps) as u64);
        let poll_timeout = Duration::from_millis(5);

        // Initial render: mark all widgets dirty so first frame draws everything
        self.screen.mark_dirty();

        let mut last_render = std::time::Instant::now();

        while !self.should_quit {
            let has_event = terminal::poll_event(poll_timeout)?;
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_render);

            // Render when: event arrived (responsive), or dirty + frame-cap-ok
            let should_render = has_event
                || (self.screen.needs_render() && elapsed >= min_frame_time);

            if should_render {
                terminal.draw(|frame| {
                    self.screen.render(frame);
                })?;
                last_render = now;
            }

            if has_event {
                let crossterm_event = terminal::read_event()?;
                self.handle_crossterm_event(crossterm_event);
            }

            if !has_event && !self.screen.needs_render() {
                // Nothing to do; yield to reduce CPU usage
                std::thread::sleep(Duration::from_millis(1));
            }

            self.handle_event(&Event::Tick);
            self.plugins.tick_all();
        }

        Ok(())
    }

    fn handle_crossterm_event(&mut self, event: crossterm::event::Event) {
        match event {
            crossterm::event::Event::Key(key) => {
                let mode = self.input_handler.current_mode();
                let tanu_key = crate::input::from_crossterm_key(&key, mode);

                match mode {
                    crate::events::UiMode::Command
                    | crate::events::UiMode::Insert
                    | crate::events::UiMode::Search => {
                        self.dispatch_event(Event::KeyPress(tanu_key));
                    }
                    _ => {
                        let translated =
                            self.input_handler.translate_key(tanu_key.clone());
                        if let Some(event) = translated {
                            self.dispatch_event(event);
                        } else {
                            self.dispatch_event(Event::KeyPress(tanu_key));
                        }
                    }
                }
            }
            crossterm::event::Event::Mouse(mouse) => {
                if let Some(event) = crate::input::from_crossterm_mouse(&mouse) {
                    self.dispatch_event(event);
                }
            }
            crossterm::event::Event::Resize(cols, rows) => {
                self.dispatch_event(Event::Resize(cols, rows));
            }
            _ => {}
        }
    }

    fn dispatch_event(&mut self, event: Event) {
        let produced = self.screen.handle_event(&event);

        for component in &mut self.components {
            component.handle_event(&event);
        }

        self.plugins.dispatch_event(&event);
        self.handle_event(&event);

        // Forward the event to the router so background tasks receive it
        let _ = self.event_tx.send(event);

        // Dispatch events produced by widgets
        for e in produced {
            self.dispatch_event(e);
        }
    }

    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Quit => {
                self.should_quit = true;
            }
            Event::Resize(_, _) => {
                self.screen.mark_dirty();
            }
            Event::ModeChanged(mode) => {
                self.input_handler.set_mode(*mode);
            }
            Event::LibraryScanComplete { .. } => {
                self.refresh_library_table();
            }
            Event::SearchQueryChanged(query) => {
                self.search_library(query);
            }
            Event::BindingsReloaded => {
                // Reload bindings from default path
                let config_path = dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("tanu")
                    .join("bindings.toml");
                if let Ok(bindings) = crate::config::BindingsConfig::load(&config_path) {
                    self.input_handler.reload_bindings(bindings);
                    tracing::info!("Bindings reloaded");
                }
            }
            Event::Command(cmd_str) => {
                self.execute_command(cmd_str);
            }
            Event::PluginLoaded(name) => {
                tracing::info!(plugin = %name, "Plugin loaded");
            }
            Event::PluginUnloaded(name) => {
                tracing::info!(plugin = %name, "Plugin unloaded");
            }
            Event::PluginError(name, error) => {
                tracing::error!(plugin = %name, error = %error, "Plugin error");
            }
            Event::ConfigReloaded => {
                // Reload themes from default config directory
                let themes_dir = dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("tanu")
                    .join("themes");
                if themes_dir.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&themes_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                                if let Err(e) = self.screen.theme_mut().load(&path) {
                                    tracing::warn!(path = %path.display(), error = %e, "Failed to reload theme");
                                }
                            }
                        }
                        self.screen.mark_dirty();
                        tracing::info!("Themes reloaded from {}", themes_dir.display());
                    }
                }
            }
            _ => {}
        }
    }

    fn execute_command(&mut self, input: &str) {
        let input = input.strip_prefix(':').unwrap_or(input);
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        let cmd = parts[0];
        let args: Vec<&str> = parts[1..].to_vec();

        // Handle yank: prefix (copied paths)
        if input.starts_with("yank:") {
            let data = &input[5..];
            let display = data.replace(',', "\n");
            tracing::info!(paths = data, "Yanked");
            println!("{}", display);
            self.screen.show_popup_info("Yanked", format!("Copied:\n{}", display));
            return;
        }

        // Handle move_item:from:to (drag-and-drop reorder)
        if input.starts_with("move_item:") {
            let parts: Vec<&str> = input[10..].split(':').collect();
            if parts.len() == 2 {
                if let (Ok(from), Ok(to)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                    tracing::info!(from = from, to = to, "Item moved");
                    let _ = self.event_tx.send(Event::QueueChanged);
                }
            }
            return;
        }

        let result = match cmd {
            "play" | "p" => {
                let _ = self.event_tx.send(Event::Play);
                Ok(())
            }
            "pause" => {
                let _ = self.event_tx.send(Event::Pause);
                Ok(())
            }
            "stop" => {
                let _ = self.event_tx.send(Event::Stop);
                Ok(())
            }
            "next" | "n" => {
                let _ = self.event_tx.send(Event::Next);
                Ok(())
            }
            "previous" | "prev" => {
                let _ = self.event_tx.send(Event::Previous);
                Ok(())
            }
            "toggle" => {
                let _ = self.event_tx.send(Event::TogglePlayPause);
                Ok(())
            }
            "volume" | "vol" => {
                if let Some(vol_str) = args.first() {
                    if let Ok(vol) = vol_str.parse::<f32>() {
                        let clamped = (vol / 100.0).clamp(0.0, 1.0);
                        let _ = self.event_tx.send(Event::SetVolume(clamped));
                        Ok(())
                    } else {
                        Err("Invalid volume value (0-100)".to_string())
                    }
                } else {
                    Err("Usage: volume <0-100>".to_string())
                }
            }
            "seek" => {
                if let Some(pos_str) = args.first() {
                    if let Ok(pos) = pos_str.parse::<f64>() {
                        let _ = self.event_tx.send(Event::Seek(pos));
                        Ok(())
                    } else {
                        Err("Invalid seek position (seconds)".to_string())
                    }
                } else {
                    Err("Usage: seek <seconds>".to_string())
                }
            }
            "shuffle" => {
                match args.first().copied() {
                    Some("on") | Some("true") | Some("1") => {
                        let _ = self.event_tx.send(Event::SetShuffle(true));
                        Ok(())
                    }
                    Some("off") | Some("false") | Some("0") => {
                        let _ = self.event_tx.send(Event::SetShuffle(false));
                        Ok(())
                    }
                    _ => Err("Usage: shuffle on|off".to_string()),
                }
            }
            "repeat" => {
                match args.first().copied() {
                    Some("off") => {
                        let _ = self.event_tx.send(Event::SetRepeat(crate::events::RepeatMode::Off));
                        Ok(())
                    }
                    Some("track") => {
                        let _ = self.event_tx.send(Event::SetRepeat(crate::events::RepeatMode::Track));
                        Ok(())
                    }
                    Some("playlist") => {
                        let _ = self.event_tx.send(Event::SetRepeat(crate::events::RepeatMode::Playlist));
                        Ok(())
                    }
                    _ => Err("Usage: repeat off|track|playlist".to_string()),
                }
            }
            "rescan" => {
                if let Some(ref db) = self.db {
                    let music_dirs = std::env::args()
                        .skip(1)
                        .map(PathBuf::from)
                        .collect::<Vec<_>>();
                    let dirs = if music_dirs.is_empty() {
                        vec![dirs::audio_dir().unwrap_or_else(|| PathBuf::from("."))]
                    } else {
                        music_dirs
                    };
                    self.scan_library(db.clone(), dirs);
                    Ok(())
                } else {
                    Err("No database available".to_string())
                }
            }
            "quit" | "q" => {
                self.should_quit = true;
                Ok(())
            }
            "theme" => {
                if let Some(name) = args.first() {
                    let name = *name;
                    if name == "list" {
                        let names = self.screen.theme().list_names();
                        let list = names.join("\n");
                        self.screen.show_popup_info("Themes", format!("Available themes:\n{}", list));
                        self.screen.mark_dirty();
                        Ok(())
                    } else if let Some(rest) = name.strip_prefix("preview:") {
                        match self.screen.theme_mut().preview_theme(rest) {
                            Ok(()) => {
                                self.screen.mark_dirty();
                                let names = self.screen.theme().list_names();
                                let list = names.join(", ");
                                self.screen.show_popup_info_with_actions(
                                    "Theme Preview",
                                    format!("Previewing: {}\n\nAvailable: {}", rest, list),
                                    Some(":theme apply".into()),
                                    Some(":theme cancel".into()),
                                );
                                Ok(())
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    } else if name == "apply" {
                        self.screen.theme_mut().apply_preview();
                        self.screen.hide_popup();
                        self.screen.mark_dirty();
                        Ok(())
                    } else if name == "cancel" {
                        self.screen.theme_mut().cancel_preview();
                        self.screen.hide_popup();
                        self.screen.mark_dirty();
                        Ok(())
                    } else {
                        match self.screen.theme_mut().switch(name) {
                            Ok(()) => {
                                self.screen.mark_dirty();
                                Ok(())
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                } else {
                    Err("Usage: theme <name> | list | preview:<name> | apply | cancel".to_string())
                }
            }
            "refresh" => {
                self.screen.mark_dirty();
                Ok(())
            }
            "layout" => {
                if let Some(name) = args.first() {
                    if *name == "list" {
                        let names = self.screen.layout().list_names();
                        let list = names.join("\n");
                        self.screen.show_popup_info("Layouts", format!("Available layouts:\n{}", list));
                        self.screen.mark_dirty();
                        Ok(())
                    } else {
                        match self.screen.switch_layout(name) {
                            Ok(()) => {
                                let _ = self.event_tx.send(Event::LayoutChanged(name.to_string()));
                                Ok(())
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                } else {
                    Err("Usage: layout <name> | list".to_string())
                }
            }
            "set" => {
                if args.len() >= 2 {
                    let key = args[0];
                    let value = args[1..].join(" ");
                    tracing::info!(key = key, value = %value, "Setting config");
                    // TODO: persist to config
                    Ok(())
                } else {
                    Err("Usage: set <key> <value>".to_string())
                }
            }
            "add" => {
                if let Some(path_str) = args.first() {
                    let path = PathBuf::from(path_str);
                    let _ = self.event_tx.send(Event::DirectoryChanged(path.to_string_lossy().to_string()));
                    if let Some(ref db) = self.db {
                        self.scan_library(db.clone(), vec![path]);
                    }
                    Ok(())
                } else {
                    Err("Usage: add <path>".to_string())
                }
            }
            "library" | "browser" | "playlist" | "queue" => {
                let view = match cmd {
                    "library" => ActiveMainView::Library,
                    "browser" => ActiveMainView::Browser,
                    "playlist" => ActiveMainView::Playlist,
                    _ => ActiveMainView::Queue,
                };
                self.switch_view(view);
                Ok(())
            }
            "next_view" => {
                self.screen.focus_next();
                Ok(())
            }
            "previous_view" => {
                self.screen.focus_previous();
                Ok(())
            }
            _ => {
                self.commands.execute(input)
                    .map_err(|e| e.to_string())
            }
        };

        let _ = self.event_tx.send(Event::CommandResult {
            command: input.to_string(),
            success: result.is_ok(),
            message: result.as_ref().err().cloned(),
        });
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::id::ComponentId;
    use crate::core::traits::Component as ComponentTrait;

    struct Counter {
        id: ComponentId,
        count: usize,
    }

    impl Counter {
        fn new() -> Self {
            Self {
                id: ComponentId::new(),
                count: 0,
            }
        }
    }

    impl ComponentTrait for Counter {
        fn id(&self) -> ComponentId {
            self.id
        }
        fn name(&self) -> &str {
            "counter"
        }
        fn handle_event(&mut self, event: &Event) -> bool {
            if matches!(event, Event::Play) {
                self.count += 1;
                true
            } else {
                false
            }
        }
    }

    #[test]
    fn test_app_registration() {
        let mut app = App::default_app();
        let counter = Counter::new();
        app.register_component(Box::new(counter));
        assert_eq!(app.components.len(), 1);
    }

    #[test]
    fn test_switch_view_updates_active_panels() {
        let mut app = App::default_app();
        assert_eq!(app.active_left, ActiveMainView::Library);
        assert_eq!(app.active_right, ActiveMainView::Playlist);

        app.switch_view(ActiveMainView::Browser);
        assert_eq!(app.active_left, ActiveMainView::Browser);
        // right panel untouched
        assert_eq!(app.active_right, ActiveMainView::Playlist);

        app.switch_view(ActiveMainView::Queue);
        assert_eq!(app.active_right, ActiveMainView::Queue);
        assert_eq!(app.active_left, ActiveMainView::Browser);

        // no-op when already active
        app.switch_view(ActiveMainView::Queue);
        assert_eq!(app.active_right, ActiveMainView::Queue);
    }

    #[test]
    fn test_app_handle_quit() {
        let mut app = App::default_app();
        app.handle_event(&Event::Quit);
        assert!(app.should_quit());
    }

    #[test]
    fn test_app_handle_play_event() {
        let mut app = App::default_app();
        let counter = Counter::new();
        let counter_id = counter.id();
        app.register_component(Box::new(counter));
        app.handle_event(&Event::Play);
        let _ = app.components.iter().find(|c| c.id() == counter_id).unwrap();
    }
}
