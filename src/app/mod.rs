//! Application runtime — owns the terminal, screen, and input handler.
//! Runs the main event loop: poll crossterm events, translate, dispatch, render.

use std::any::Any;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crate::audio::backend::RodioBackend;
use crate::commands::CommandRegistry;
use crate::core::traits::Command;
use crate::core::traits::Component;
use crate::database::Database;
use crate::events::bus::{self, EventRouter, EventSender};
use crate::events::Event;
use crate::input::InputHandler;
use crate::library::Library;
use crate::player::{Player, PlayerCommand};
use crate::plugins::PluginManager;
use crate::ui::{Screen, Slot};
use crate::widgets::browser_view::BrowserView;
use crate::widgets::command_bar::CommandBar;
use crate::widgets::library_view::LibraryView;
use crate::widgets::progress_bar::ProgressBar;
use crate::widgets::status_bar::StatusBar;
use crate::widgets::Widget;

use notify::{EventKind, RecursiveMode, Watcher};

pub mod terminal;

use self::terminal::Terminal;

/// The main application struct. Owns all top-level subsystems and
/// drives the render loop.
pub struct App {
    screen: Screen,
    input_handler: InputHandler,
    commands: CommandRegistry,
    event_tx: EventSender,
    router: Option<EventRouter>,
    /// Receives events broadcast by the router (player state, etc.) for the UI.
    ui_rx: bus::EventReceiver,
    components: Vec<Box<dyn Component>>,
    plugins: PluginManager,
    db: Option<Database>,
    should_quit: bool,
    mouse: crate::mouse::MouseHandler,
    viz: crate::audio::viz::AudioViz,
    eq: crate::audio::eq::EqState,
    /// User-selected MIDI SoundFont, shared with the audio backend.
    soundfont: crate::audio::backend::SharedSoundFont,
    /// Latest playback state (from PlayerStateChanged) for smart key handling.
    playing: bool,
    volume: f32,
    /// Path of the loaded track, tracked from PlayerStateChanged.
    current_track: Option<String>,
}

impl App {
    pub fn new(screen: Screen, input_handler: InputHandler, commands: CommandRegistry) -> Self {
        let mut router = EventRouter::new();
        let event_tx = router.sender();
        // Feed router events (e.g. PlayerStateChanged from the player thread)
        // back into the UI so widgets update.
        let (ui_tx, ui_rx) = bus::event_channel();
        router.register_listener(ui_tx);
        let plugins = PluginManager::new(crate::plugins::PluginContext::new(event_tx.clone()));
        Self {
            screen,
            input_handler,
            commands,
            event_tx,
            router: Some(router),
            ui_rx,
            components: Vec::new(),
            plugins,
            db: None,
            should_quit: false,
            mouse: crate::mouse::MouseHandler::new(),
            viz: crate::audio::viz::AudioViz::new(),
            eq: crate::audio::eq::EqState::new(),
            soundfont: std::sync::Arc::new(std::sync::Mutex::new(Self::load_soundfont())),
            playing: false,
            volume: 0.8,
            current_track: None,
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

        // Restore saved theme settings (theme name + accent/typography color).
        let cfg = crate::config::Config::load_or_default(&Self::config_file_path());
        let mut theme = ThemeRegistry::new();
        let _ = theme.switch(&cfg.ui.theme); // ignore if the name is unknown
        if let Some(ref hex) = cfg.ui.text_color {
            crate::theme::set_primary_hex(hex);
        }
        let mut screen = Screen::new(theme);

        let menu_bar = crate::widgets::menu_bar::MenuBar::new();
        screen.add_widget(Box::new(menu_bar), Slot::Tabs);

        // Album art box (right column, top). Reuses the SearchBar slot.
        let album_art = crate::widgets::album_art::AlbumArt::new();
        screen.add_widget(Box::new(album_art), Slot::SearchBar);

        // Main panel: file browser, rooted at the saved library folder.
        let browser = BrowserView::new(Self::library_start_dir());
        let browser_id = browser.id();
        screen.add_widget(Box::new(browser), Slot::MainLeft);
        screen.set_focus(Some(browser_id));

        // Shared audio buffer feeds the visualizer; shared EQ state drives the
        // graphic equalizer (which modifies the sound).
        let viz = crate::audio::viz::AudioViz::new();
        let eq_state = crate::audio::eq::EqState::new();
        let eq = crate::widgets::equalizer::Equalizer::new(eq_state.clone());
        screen.add_widget(Box::new(eq), Slot::Eq);
        let scope = crate::widgets::oscilloscope::Oscilloscope::new(viz.clone());
        screen.add_widget(Box::new(scope), Slot::MainRight);

        let seek = crate::widgets::seek_bar::SeekBar::new();
        screen.add_widget(Box::new(seek), Slot::Seek);

        let transport = ProgressBar::new();
        screen.add_widget(Box::new(transport), Slot::ProgressBar);

        let status = StatusBar::new();
        screen.add_widget(Box::new(status), Slot::StatusBar);

        let cmd_bar = CommandBar::new();
        screen.add_widget(Box::new(cmd_bar), Slot::CommandBar);

        let input_handler = InputHandler::new(BindingsConfig::default_bindings());
        let commands = CommandRegistry::new();

        let mut app = Self::new(screen, input_handler, commands);
        app.viz = viz;
        app.eq = eq_state;
        app
    }

    /// Path to the persisted config file.
    fn config_file_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tanu")
            .join("config.toml")
    }

    /// Directory the browser starts in: the saved library folder, else audio dir.
    fn library_start_dir() -> PathBuf {
        let cfg = crate::config::Config::load_or_default(&Self::config_file_path());
        cfg.library
            .music_dirs
            .into_iter()
            .find(|p| p.is_dir())
            .unwrap_or_else(|| dirs::audio_dir().unwrap_or_else(|| PathBuf::from(".")))
    }

    /// Apply and persist a `:set <key> <value>` change to the config file.
    fn set_config(&mut self, key: &str, value: &str) -> Result<(), String> {
        let cfg_path = Self::config_file_path();
        let mut cfg = crate::config::Config::load_or_default(&cfg_path);
        match key {
            "theme" => {
                self.screen
                    .theme_mut()
                    .switch(value)
                    .map_err(|e| e.to_string())?;
                self.screen.mark_dirty();
                cfg.ui.theme = value.to_string();
            }
            "volume" | "vol" => {
                let v: f32 = value
                    .parse()
                    .map_err(|_| "volume must be 0-100".to_string())?;
                let clamped = (v / 100.0).clamp(0.0, 1.0);
                let _ = self.event_tx.send(Event::SetVolume(clamped));
                cfg.audio.default_volume = clamped;
            }
            "library" | "library_dir" => {
                let pb = PathBuf::from(value);
                if !pb.is_dir() {
                    return Err(format!("not a directory: {}", value));
                }
                self.set_browser_dir(pb.clone());
                cfg.library.music_dirs = vec![pb];
            }
            "max_fps" => {
                let fps: u32 = value
                    .parse()
                    .map_err(|_| "max_fps must be a number".to_string())?;
                cfg.ui.max_fps = fps.clamp(10, 240);
            }
            other => return Err(format!("unknown config key: {}", other)),
        }
        if let Some(parent) = cfg_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        cfg.save(&cfg_path).map_err(|e| e.to_string())?;
        self.screen
            .show_popup_info("Config", format!("{} = {} (saved)", key, value));
        Ok(())
    }

    /// Persist the library start directory to the config file.
    fn save_library_dir(path: &std::path::Path) {
        let cfg_path = Self::config_file_path();
        let mut cfg = crate::config::Config::load_or_default(&cfg_path);
        cfg.library.music_dirs = vec![path.to_path_buf()];
        if let Some(parent) = cfg_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = cfg.save(&cfg_path) {
            tracing::warn!(error = %e, "Failed to save library dir");
        }
    }

    /// Persist the selected theme name to the config file.
    fn save_theme_name(name: &str) {
        let cfg_path = Self::config_file_path();
        let mut cfg = crate::config::Config::load_or_default(&cfg_path);
        cfg.ui.theme = name.to_string();
        if let Some(parent) = cfg_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = cfg.save(&cfg_path) {
            tracing::warn!(error = %e, "Failed to save theme");
        }
    }

    /// Path of the persisted MIDI SoundFont pointer.
    fn soundfont_file_path() -> std::path::PathBuf {
        Self::config_file_path()
            .parent()
            .map(|p| p.join("soundfont.txt"))
            .unwrap_or_else(|| std::path::PathBuf::from("soundfont.txt"))
    }

    /// Load the persisted SoundFont selection, if any and still present.
    fn load_soundfont() -> Option<std::path::PathBuf> {
        let p = std::fs::read_to_string(Self::soundfont_file_path()).ok()?;
        let pb = std::path::PathBuf::from(p.trim());
        pb.is_file().then_some(pb)
    }

    /// Persist the chosen SoundFont path.
    fn save_soundfont(path: &std::path::Path) {
        let f = Self::soundfont_file_path();
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&f, path.to_string_lossy().as_bytes()) {
            tracing::warn!(error = %e, "Failed to save soundfont");
        }
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
                // Play a specific file: replace the queue with it and play.
                if let Event::PlayPath(path) = &event {
                    let p = PathBuf::from(path);
                    if cmd_tx_clone
                        .send(PlayerCommand::SetQueue(vec![p], 0))
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
                // Play a directory's media files, starting at `index`.
                if let Event::PlayQueue(paths, index) = &event {
                    let ps: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
                    if cmd_tx_clone
                        .send(PlayerCommand::SetQueue(ps, *index))
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
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
        let viz = self.viz.clone();
        let eq = self.eq.clone();
        let soundfont = self.soundfont.clone();
        std::thread::spawn(move || {
            let backend = match RodioBackend::new(viz, eq, soundfont) {
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

    /// Load the given track's embedded cover into the album-art panel.
    fn update_album_art(&mut self, path: &std::path::Path) {
        if let Some(w) = self.screen.widget_at_mut(Slot::SearchBar) {
            let any = w.as_mut() as &mut dyn Any;
            if let Some(art) = any.downcast_mut::<crate::widgets::album_art::AlbumArt>() {
                art.set_track(&path.to_path_buf());
            }
        }
    }

    /// Move the explorer cursor onto the now-playing track.
    fn follow_now_playing(&mut self, path: &std::path::Path) {
        if let Some(w) = self.screen.widget_at_mut(Slot::MainLeft) {
            let any = w.as_mut() as &mut dyn Any;
            if let Some(browser) = any.downcast_mut::<BrowserView>() {
                browser.select_path(path);
            }
        }
    }

    /// Show "artist / title" (from tags, else the filename) in the status bar.
    fn set_now_playing_status(&mut self, path: &std::path::Path) {
        let text = now_playing_text(path);
        if let Some(w) = self.screen.widget_at_mut(Slot::StatusBar) {
            let any = w.as_mut() as &mut dyn Any;
            if let Some(sb) = any.downcast_mut::<crate::widgets::status_bar::StatusBar>() {
                sb.set_now_playing(text.clone());
            }
        }
        if let Some(w) = self.screen.widget_at_mut(Slot::Seek) {
            let any = w.as_mut() as &mut dyn Any;
            if let Some(sb) = any.downcast_mut::<crate::widgets::seek_bar::SeekBar>() {
                sb.set_now_playing(text);
            }
        }
    }

    /// Ordered media files of the selected file's directory + its index,
    /// so «/» navigate within that directory.
    fn selected_media_queue(&mut self) -> Option<(Vec<String>, usize)> {
        let widget = self.screen.widget_at_mut(Slot::MainLeft)?;
        let any = widget.as_mut() as &mut dyn Any;
        let browser = any.downcast_mut::<BrowserView>()?;
        browser.selected_media_queue()
    }

    /// Point the browser at a new directory (library folder change).
    fn set_browser_dir(&mut self, path: PathBuf) {
        if let Some(widget) = self.screen.widget_at_mut(Slot::MainLeft) {
            let any = widget.as_mut() as &mut dyn Any;
            if let Some(browser) = any.downcast_mut::<BrowserView>() {
                browser.navigate_to(path);
                self.screen.mark_dirty();
            }
        }
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

            while let Ok(event) = rx.recv() {
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
    pub fn register_plugin(
        &mut self,
        plugin: Box<dyn crate::core::traits::Plugin>,
    ) -> anyhow::Result<()> {
        self.plugins.register(plugin)
    }

    /// Register all built-in plugins.
    pub fn register_builtin_plugins(&mut self) {
        use crate::plugins::builtin::discord::DiscordPresence;
        use crate::plugins::builtin::lyrics::LyricsPlugin;
        use crate::plugins::builtin::scrobbler::LastFmScrobbler;

        self.plugins
            .register(Box::new(LastFmScrobbler::new()))
            .unwrap_or_else(|e| tracing::warn!("Failed to register scrobbler: {}", e));
        self.plugins
            .register(Box::new(DiscordPresence::new()))
            .unwrap_or_else(|e| tracing::warn!("Failed to register discord: {}", e));
        self.plugins
            .register(Box::new(LyricsPlugin::new()))
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

            while let Ok(event) = rx.recv() {
                let has_toml = event
                    .paths
                    .iter()
                    .any(|p| p.extension().map(|e| e == "toml").unwrap_or(false));
                if has_toml && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    let _ = event_tx.send(Event::ConfigReloaded);
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
            let should_render =
                has_event || (self.screen.needs_render() && elapsed >= min_frame_time);

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

            // Drain events broadcast by the router. The router echoes back
            // everything the UI already dispatched, so only act on events that
            // originate off-thread (player state, library scans).
            while let Ok(ev) = self.ui_rx.try_recv() {
                match ev {
                    Event::PlayerStateChanged(ref state) => {
                        self.playing = state.is_playing;
                        self.volume = state.volume;
                        // Track changed (next/prev/auto-advance): refresh art,
                        // follow the tree cursor, and update the status bar.
                        if state.current_path != self.current_track {
                            self.current_track = state.current_path.clone();
                            if let Some(p) = self.current_track.clone() {
                                let path = PathBuf::from(&p);
                                self.update_album_art(&path);
                                self.follow_now_playing(&path);
                                self.set_now_playing_status(&path);
                            }
                        }
                        let _ = self.screen.handle_event(&ev);
                    }
                    Event::LibraryScanStarted
                    | Event::LibraryScanProgress { .. }
                    | Event::LibraryScanComplete { .. } => {
                        self.handle_event(&ev);
                    }
                    _ => {}
                }
            }

            if !has_event && !self.screen.needs_render() {
                // Nothing to do; yield to reduce CPU usage
                std::thread::sleep(Duration::from_millis(1));
            }

            self.handle_event(&Event::Tick);
            // Let widgets animate (oscilloscope self-throttles to ~25fps).
            let _ = self.screen.handle_event(&Event::Tick);
            self.plugins.tick_all();
        }

        Ok(())
    }

    fn handle_crossterm_event(&mut self, event: crossterm::event::Event) {
        match event {
            crossterm::event::Event::Key(key) => {
                // Ctrl+C / Ctrl+Q always quit, regardless of mode.
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
                    && matches!(
                        key.code,
                        crossterm::event::KeyCode::Char('c') | crossterm::event::KeyCode::Char('q')
                    )
                {
                    self.should_quit = true;
                    return;
                }

                let mode = self.input_handler.current_mode();
                let tanu_key = crate::input::from_crossterm_key(&key, mode);

                // A visible input popup needs raw keys, otherwise Normal-mode
                // bindings (j/k/g/...) would hijack typed path characters.
                if self.screen.popup_visible() {
                    self.dispatch_event(Event::KeyPress(tanu_key));
                    return;
                }

                match mode {
                    crate::events::UiMode::Command
                    | crate::events::UiMode::Insert
                    | crate::events::UiMode::Search => {
                        self.dispatch_event(Event::KeyPress(tanu_key));
                    }
                    _ => {
                        let translated = self.input_handler.translate_key(tanu_key.clone());
                        if let Some(event) = translated {
                            self.dispatch_event(event);
                        } else {
                            self.dispatch_event(Event::KeyPress(tanu_key));
                        }
                    }
                }
            }
            crossterm::event::Event::Mouse(mouse) => {
                use crossterm::event::MouseEventKind;
                let (x, y) = (mouse.column, mouse.row);
                // Route through MouseHandler so double-click / right-click are
                // detected (raw crossterm only gives Down/Up/Drag/Scroll).
                let action = match mouse.kind {
                    MouseEventKind::Down(btn) => Some(self.mouse.on_press(
                        crate::input::convert_mouse_button(btn),
                        x,
                        y,
                    )),
                    MouseEventKind::Up(btn) => Some(self.mouse.on_release(
                        crate::input::convert_mouse_button(btn),
                        x,
                        y,
                    )),
                    MouseEventKind::Drag(_) => Some(self.mouse.on_move(x, y)),
                    MouseEventKind::Moved => Some(self.mouse.on_move(x, y)),
                    MouseEventKind::ScrollUp => Some(self.mouse.on_scroll_up(x, y)),
                    MouseEventKind::ScrollDown => Some(self.mouse.on_scroll_down(x, y)),
                    MouseEventKind::ScrollLeft => Some(self.mouse.on_scroll_left(x, y)),
                    MouseEventKind::ScrollRight => Some(self.mouse.on_scroll_right(x, y)),
                };
                if let Some(action) = action {
                    self.dispatch_event(Event::MouseAction(action));
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
                // Drive the explorer's incremental search from Search mode.
                if let Some(widget) = self.screen.widget_at_mut(Slot::MainLeft) {
                    let any = widget.as_mut() as &mut dyn Any;
                    if let Some(browser) = any.downcast_mut::<BrowserView>() {
                        match mode {
                            crate::events::UiMode::Search => browser.start_search(),
                            _ => browser.end_search(),
                        }
                    }
                }
                self.screen.mark_dirty();
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
        if let Some(data) = input.strip_prefix("yank:") {
            let display = data.replace(',', "\n");
            tracing::info!(paths = data, "Yanked");
            self.screen
                .show_popup_info("Yanked", format!("Copied:\n{}", display));
            return;
        }

        // Handle move_item:from:to (drag-and-drop reorder)
        if let Some(rest) = input.strip_prefix("move_item:") {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(from), Ok(to)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                    tracing::info!(from = from, to = to, "Item moved");
                    let _ = self.event_tx.send(Event::QueueChanged);
                }
            }
            return;
        }

        // Play a file chosen in the browser or via the Open File dialog.
        if let Some(path) = input.strip_prefix("play_file:") {
            let path = path.trim();
            if path.is_empty() {
                return;
            }
            tracing::info!(path = path, "Play file");
            let _ = self.event_tx.send(Event::PlayPath(path.to_string()));
            self.update_album_art(std::path::Path::new(path));
            let name = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string());
            self.screen.show_popup_info("Playing", name);
            return;
        }

        // Folder chosen in the tree picker: point the browser at it AND scan it
        // (library folder + scan are the same action now).
        if let Some(path) = input.strip_prefix("pick_dir:") {
            let path = path.trim();
            let pb = PathBuf::from(path);
            if !pb.is_dir() {
                self.screen
                    .show_popup_error("Invalid folder", format!("Not a directory:\n{}", path));
                return;
            }
            Self::save_library_dir(&pb);
            self.set_browser_dir(pb.clone());
            let db = self.db.clone().or_else(|| {
                let db_path = dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("tanu")
                    .join("tanu.db");
                Database::open(&db_path).ok()
            });
            if let Some(db) = db {
                self.scan_library(db, vec![pb]);
            }
            self.screen
                .show_popup_info("Scan Folder", format!("Browsing & indexing\n{}", path));
            return;
        }

        // Scan a directory into the library (from the Scan Directory dialog).
        if let Some(path) = input.strip_prefix("scan_path:") {
            let path = path.trim();
            if path.is_empty() {
                return;
            }
            let pb = PathBuf::from(path);
            let db = self.db.clone().or_else(|| {
                let db_path = dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("tanu")
                    .join("tanu.db");
                Database::open(&db_path).ok()
            });
            if let Some(db) = db {
                self.scan_library(db, vec![pb]);
                self.screen
                    .show_popup_info("Scanning", format!("Indexing {}", path));
            } else {
                self.screen
                    .show_popup_error("Scan failed", "No database available");
            }
            return;
        }

        // Library folder: persist the start directory and point the browser at it.
        if let Some(path) = input.strip_prefix("library_dir:") {
            let path = path.trim();
            if path.is_empty() {
                return;
            }
            let pb = PathBuf::from(path);
            if pb.is_dir() {
                Self::save_library_dir(&pb);
                self.set_browser_dir(pb);
                self.screen.show_popup_info(
                    "Library Folder",
                    format!("Start directory set to\n{}", path),
                );
            } else {
                self.screen
                    .show_popup_error("Invalid folder", format!("Not a directory:\n{}", path));
            }
            return;
        }

        // Sound source / output device (EDIT menu). rodio picks the default
        // output; switching devices at runtime is not wired to the backend yet.
        // ponytail: records the choice + acks; wire to backend device selection when needed.
        if let Some(val) = input.strip_prefix("sound_source:") {
            let val = val.trim();
            tracing::info!(source = val, "Sound source requested");
            self.screen.show_popup_info(
                "Sound Source",
                format!("Selected: {}\n(applies to the default output)", val),
            );
            return;
        }

        // MIDI SoundFont chosen in the file picker (EDIT → SoundFont).
        if let Some(path) = input.strip_prefix("set_soundfont:") {
            let pb = PathBuf::from(path.trim());
            if pb.is_file() {
                *self.soundfont.lock().unwrap() = Some(pb.clone());
                Self::save_soundfont(&pb);
                self.screen.show_popup_info(
                    "SoundFont",
                    format!("MIDI will use:\n{}", pb.to_string_lossy()),
                );
            } else {
                self.screen
                    .show_popup_error("SoundFont", format!("Not a file:\n{}", path));
            }
            return;
        }

        // EQ preset picker: open a centered modal listing the Winamp presets.
        if input == "eq_presets" {
            let items: Vec<crate::widgets::context_menu::MenuItem> = crate::audio::eq::PRESETS
                .iter()
                .enumerate()
                .map(|(i, (name, _))| crate::widgets::context_menu::MenuItem {
                    label: (*name).to_string(),
                    command: format!("eq_preset:{}", i),
                })
                .collect();
            self.screen.show_modal_menu("EQ Preset", items);
            self.screen.mark_dirty();
            return;
        }
        // Apply a chosen EQ preset to the equalizer widget.
        if let Some(idx) = input
            .strip_prefix("eq_preset:")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some(w) = self.screen.widget_at_mut(Slot::Eq) {
                let any = w.as_mut() as &mut dyn Any;
                if let Some(eq) = any.downcast_mut::<crate::widgets::equalizer::Equalizer>() {
                    eq.apply_preset(idx);
                }
            }
            self.screen.mark_dirty();
            return;
        }

        // Typography color (EDIT → Text Color). Sets the global primary/accent
        // color used by panel titles and the brand; redraw everything.
        if let Some(hex) = input.strip_prefix("text_color:") {
            let hex = hex.trim();
            if crate::theme::set_primary_hex(hex) {
                self.screen.mark_dirty();
                // Persist so the choice survives restarts.
                let cfg_path = Self::config_file_path();
                let mut cfg = crate::config::Config::load_or_default(&cfg_path);
                cfg.ui.text_color = Some(hex.to_string());
                if let Some(parent) = cfg_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = cfg.save(&cfg_path) {
                    tracing::warn!(error = %e, "Failed to save text color");
                }
            }
            return;
        }

        // Open a dropdown menu from the menu bar: "menu:<name>:<x>".
        if let Some(rest) = input.strip_prefix("menu:") {
            let mut it = rest.splitn(2, ':');
            let name = it.next().unwrap_or("");
            let x: u16 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let items = match name {
                "file" => vec![
                    crate::widgets::context_menu::MenuItem {
                        label: "Open File...".into(),
                        command: "open_file".into(),
                    },
                    crate::widgets::context_menu::MenuItem {
                        label: "Scan Folder...".into(),
                        command: "scan_dir".into(),
                    },
                    crate::widgets::context_menu::MenuItem {
                        label: "Quit".into(),
                        command: "quit".into(),
                    },
                ],
                "edit" => vec![
                    crate::widgets::context_menu::MenuItem {
                        label: "Sound Source...".into(),
                        command: "set_source".into(),
                    },
                    crate::widgets::context_menu::MenuItem {
                        label: "SoundFont (.sf2)...".into(),
                        command: "pick_soundfont".into(),
                    },
                    crate::widgets::context_menu::MenuItem {
                        label: "Text Color...".into(),
                        command: format!("menu:color:{}", x),
                    },
                ],
                "color" => crate::theme::PRIMARY_PALETTE
                    .iter()
                    .map(|(name, hex)| crate::widgets::context_menu::MenuItem {
                        label: (*name).to_string(),
                        command: format!("text_color:{}", hex),
                    })
                    .collect(),
                _ => vec![],
            };
            if !items.is_empty() {
                if name == "color" {
                    // Palette picker is a centered modal with colored swatches.
                    self.screen.show_modal_menu("Text Color", items);
                } else {
                    self.screen.show_context_menu(x, 1, items);
                }
                self.screen.mark_dirty();
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
            "shuffle" => match args.first().copied() {
                Some("on") | Some("true") | Some("1") => {
                    let _ = self.event_tx.send(Event::SetShuffle(true));
                    Ok(())
                }
                Some("off") | Some("false") | Some("0") => {
                    let _ = self.event_tx.send(Event::SetShuffle(false));
                    Ok(())
                }
                _ => Err("Usage: shuffle on|off".to_string()),
            },
            "repeat" => match args.first().copied() {
                Some("off") => {
                    let _ = self
                        .event_tx
                        .send(Event::SetRepeat(crate::events::RepeatMode::Off));
                    Ok(())
                }
                Some("track") => {
                    let _ = self
                        .event_tx
                        .send(Event::SetRepeat(crate::events::RepeatMode::Track));
                    Ok(())
                }
                Some("playlist") => {
                    let _ = self
                        .event_tx
                        .send(Event::SetRepeat(crate::events::RepeatMode::Playlist));
                    Ok(())
                }
                _ => Err("Usage: repeat off|track|playlist".to_string()),
            },
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
                        self.screen
                            .show_popup_info("Themes", format!("Available themes:\n{}", list));
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
                                Self::save_theme_name(name);
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
                        self.screen
                            .show_popup_info("Layouts", format!("Available layouts:\n{}", list));
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
                    self.set_config(key, &value)
                } else {
                    Err(
                        "Usage: set <key> <value>  (keys: theme, volume, library, max_fps)"
                            .to_string(),
                    )
                }
            }
            "add" => {
                if let Some(path_str) = args.first() {
                    let path = PathBuf::from(path_str);
                    let _ = self
                        .event_tx
                        .send(Event::DirectoryChanged(path.to_string_lossy().to_string()));
                    if let Some(ref db) = self.db {
                        self.scan_library(db.clone(), vec![path]);
                    }
                    Ok(())
                } else {
                    Err("Usage: add <path>".to_string())
                }
            }
            "smart_play_pause" => {
                if self.playing || self.current_track.is_some() {
                    // Playing → pause; stopped/paused with a loaded track → (re)play same.
                    let _ = self.event_tx.send(Event::TogglePlayPause);
                } else if let Some((paths, index)) = self.selected_media_queue() {
                    let _ = self.event_tx.send(Event::PlayQueue(paths, index));
                } else {
                    let _ = self.event_tx.send(Event::TogglePlayPause);
                }
                Ok(())
            }
            "volume_up" => {
                self.volume = (self.volume + 0.05).clamp(0.0, 1.0);
                let _ = self.event_tx.send(Event::SetVolume(self.volume));
                Ok(())
            }
            "volume_down" => {
                self.volume = (self.volume - 0.05).clamp(0.0, 1.0);
                let _ = self.event_tx.send(Event::SetVolume(self.volume));
                Ok(())
            }
            "set_volume" => {
                if let Some(v) = args.first().and_then(|s| s.parse::<f32>().ok()) {
                    self.volume = (v / 100.0).clamp(0.0, 1.0);
                    let _ = self.event_tx.send(Event::SetVolume(self.volume));
                }
                Ok(())
            }
            "library_folder" => {
                self.screen
                    .show_popup_input("Library Folder — start directory", "library_dir".into());
                self.screen.mark_dirty();
                Ok(())
            }
            "pick_soundfont" => {
                // Start where the current SoundFont lives, else home.
                let start = self
                    .soundfont
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| PathBuf::from("/"));
                self.screen.show_file_picker(
                    start,
                    vec!["sf2".into()],
                    "set_soundfont",
                    "Select SoundFont",
                );
                self.screen.mark_dirty();
                Ok(())
            }
            "open_file" => {
                self.screen
                    .show_popup_input("Open File — enter path", "play_file".into());
                self.screen.mark_dirty();
                Ok(())
            }
            "scan_dir" => {
                // Start the folder tree at the current browser root (else home / root).
                let start = self
                    .screen
                    .widget_at_mut(Slot::MainLeft)
                    .and_then(|w| {
                        (w.as_mut() as &mut dyn Any)
                            .downcast_mut::<crate::widgets::browser_view::BrowserView>()
                    })
                    .map(|b| b.current_dir().clone())
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| PathBuf::from("/"));
                self.screen.show_dir_picker(start);
                self.screen.mark_dirty();
                Ok(())
            }
            "set_source" => {
                self.screen
                    .show_popup_input("Sound Source — enter output device", "sound_source".into());
                self.screen.mark_dirty();
                Ok(())
            }
            "about" => {
                const TANU_ART: &str = include_str!("../widgets/tanu_art.txt");
                self.screen.show_popup_about(
                    "About Tanu",
                    format!(
                        "Tanu {} — a terminal music player in Rust (cmus-inspired)",
                        env!("TANU_VERSION")
                    ),
                    TANU_ART,
                );
                self.screen.mark_dirty();
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
            _ => self.commands.execute(input).map_err(|e| e.to_string()),
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

/// "artist / title" from the file's tags, falling back to the filename.
fn now_playing_text(path: &std::path::Path) -> String {
    use lofty::file::TaggedFileExt;
    use lofty::tag::Accessor;
    if let Ok(tagged) = lofty::read_from_path(path) {
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            match (tag.artist(), tag.title()) {
                (Some(a), Some(t)) => return format!("{} / {}", a, t),
                (None, Some(t)) => return t.to_string(),
                _ => {}
            }
        }
    }
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
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
    fn test_default_app_builds() {
        let app = App::default_app();
        // Browser occupies the main panel.
        assert!(app.screen.widget_at(Slot::MainLeft).is_some());
        assert!(app.screen.widget_at(Slot::MainRight).is_some());
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
        let _ = app
            .components
            .iter()
            .find(|c| c.id() == counter_id)
            .unwrap();
    }
}
