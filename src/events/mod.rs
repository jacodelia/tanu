//! Event system — the central nervous system of Tanu.
//!
//! All inter-component communication flows through typed events.
//! Each module defines its own event enum; `Event` is the global sum type.

pub mod bus;

use crate::core::id::{PlaylistId, TrackId, WidgetId};

/// The global event enum — dispatched to all subscribed components.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Quit,
    Tick,
    KeyPress(KeyEvent),
    MouseAction(MouseAction),
    Resize(u16, u16),
    Command(String),
    CommandResult {
        command: String,
        success: bool,
        message: Option<String>,
    },
    Play,
    /// Play a specific file path immediately (clears queue, enqueues, plays).
    PlayPath(String),
    Pause,
    TogglePlayPause,
    Stop,
    Next,
    Previous,
    Seek(f64),
    SetVolume(f32),
    SetShuffle(bool),
    SetRepeat(RepeatMode),
    PlayerStateChanged(PlayerState),
    LibraryScanStarted,
    LibraryScanProgress {
        tracks_found: usize,
        tracks_processed: usize,
    },
    LibraryScanComplete {
        total_tracks: usize,
        duration_secs: f64,
    },
    LibraryFilesChanged {
        added: Vec<String>,
        removed: Vec<String>,
        modified: Vec<String>,
    },
    PlaylistCreated(PlaylistId),
    PlaylistDeleted(PlaylistId),
    PlaylistModified(PlaylistId),
    TracksAddedToPlaylist(PlaylistId, Vec<TrackId>),
    TracksRemovedFromPlaylist(PlaylistId, Vec<TrackId>),
    QueueChanged,
    TrackQueued(TrackId),
    TrackDequeued(TrackId),
    FocusChanged(Option<WidgetId>),
    ModeChanged(UiMode),
    ThemeChanged(String),
    LayoutChanged(String),
    PopupOpened(String),
    PopupClosed,
    SearchQueryChanged(String),
    SearchResults(Vec<TrackId>),
    DatabaseReady,
    DatabaseError(String),
    PluginLoaded(String),
    PluginUnloaded(String),
    PluginError(String, String),
    DirectoryChanged(String),
    ConfigReloaded,
    BindingsReloaded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub mode: UiMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Tab,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    F(u8),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseAction {
    Press(MouseButton, u16, u16),
    Release(MouseButton, u16, u16),
    Hold(MouseButton, u16, u16),
    Drag(MouseButton, u16, u16),
    ScrollUp(u16, u16),
    ScrollDown(u16, u16),
    ScrollLeft(u16, u16),
    ScrollRight(u16, u16),
    DoubleClick(MouseButton, u16, u16),
    RightClick(u16, u16),
    Move(u16, u16),
}

impl MouseAction {
    /// Returns the screen coordinates (x, y) of this action.
    pub fn coords(&self) -> (u16, u16) {
        match self {
            MouseAction::Press(_, x, y)
            | MouseAction::Release(_, x, y)
            | MouseAction::Hold(_, x, y)
            | MouseAction::Drag(_, x, y)
            | MouseAction::DoubleClick(_, x, y) => (*x, *y),
            MouseAction::ScrollUp(x, y)
            | MouseAction::ScrollDown(x, y)
            | MouseAction::ScrollLeft(x, y)
            | MouseAction::ScrollRight(x, y)
            | MouseAction::RightClick(x, y)
            | MouseAction::Move(x, y) => (*x, *y),
        }
    }

    /// Returns true if this action is a click (should trigger focus).
    pub fn is_click(&self) -> bool {
        matches!(
            self,
            MouseAction::Press(..) | MouseAction::RightClick(..)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    Track,
    Playlist,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    pub track_id: Option<TrackId>,
    pub is_playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiMode {
    Normal,
    Insert,
    Command,
    Visual,
    Library,
    Browser,
    Search,
    Queue,
}

impl Event {
    pub fn event_type_name(&self) -> &'static str {
        match self {
            Event::Quit => "Quit",
            Event::Tick => "Tick",
            Event::KeyPress(_) => "KeyPress",
            Event::MouseAction(_) => "MouseAction",
            Event::Resize(_, _) => "Resize",
            Event::Command(_) => "Command",
            Event::CommandResult { .. } => "CommandResult",
            Event::Play => "Play",
            Event::PlayPath(_) => "PlayPath",
            Event::Pause => "Pause",
            Event::TogglePlayPause => "TogglePlayPause",
            Event::Stop => "Stop",
            Event::Next => "Next",
            Event::Previous => "Previous",
            Event::Seek(_) => "Seek",
            Event::SetVolume(_) => "SetVolume",
            Event::SetShuffle(_) => "SetShuffle",
            Event::SetRepeat(_) => "SetRepeat",
            Event::PlayerStateChanged(_) => "PlayerStateChanged",
            Event::LibraryScanStarted => "LibraryScanStarted",
            Event::LibraryScanProgress { .. } => "LibraryScanProgress",
            Event::LibraryScanComplete { .. } => "LibraryScanComplete",
            Event::LibraryFilesChanged { .. } => "LibraryFilesChanged",
            Event::PlaylistCreated(_) => "PlaylistCreated",
            Event::PlaylistDeleted(_) => "PlaylistDeleted",
            Event::PlaylistModified(_) => "PlaylistModified",
            Event::TracksAddedToPlaylist(_, _) => "TracksAddedToPlaylist",
            Event::TracksRemovedFromPlaylist(_, _) => "TracksRemovedFromPlaylist",
            Event::QueueChanged => "QueueChanged",
            Event::TrackQueued(_) => "TrackQueued",
            Event::TrackDequeued(_) => "TrackDequeued",
            Event::FocusChanged(_) => "FocusChanged",
            Event::ModeChanged(_) => "ModeChanged",
            Event::ThemeChanged(_) => "ThemeChanged",
            Event::LayoutChanged(_) => "LayoutChanged",
            Event::PopupOpened(_) => "PopupOpened",
            Event::PopupClosed => "PopupClosed",
            Event::SearchQueryChanged(_) => "SearchQueryChanged",
            Event::SearchResults(_) => "SearchResults",
            Event::DatabaseReady => "DatabaseReady",
            Event::DatabaseError(_) => "DatabaseError",
            Event::PluginLoaded(_) => "PluginLoaded",
            Event::PluginUnloaded(_) => "PluginUnloaded",
            Event::PluginError(_, _) => "PluginError",
            Event::DirectoryChanged(_) => "DirectoryChanged",
            Event::ConfigReloaded => "ConfigReloaded",
            Event::BindingsReloaded => "BindingsReloaded",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_types_have_names() {
        let event = Event::Play;
        assert_eq!(event.event_type_name(), "Play");
    }

    #[test]
    fn test_key_event_creation() {
        let evt = KeyEvent {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
            },
            mode: UiMode::Normal,
        };
        assert_eq!(evt.code, KeyCode::Char('j'));
    }

    #[test]
    fn test_player_state_defaults() {
        let state = PlayerState {
            track_id: None,
            is_playing: false,
            position_secs: 0.0,
            duration_secs: 0.0,
            volume: 1.0,
            shuffle: false,
            repeat: RepeatMode::Off,
        };
        assert!(!state.is_playing);
        assert_eq!(state.volume, 1.0);
    }

    #[test]
    fn test_mouse_scroll_event() {
        let evt = MouseAction::ScrollUp(10, 5);
        assert!(matches!(evt, MouseAction::ScrollUp(10, 5)));
    }

    #[test]
    fn test_command_result_format() {
        let evt = Event::CommandResult {
            command: ":play".to_string(),
            success: true,
            message: None,
        };
        assert!(matches!(evt, Event::CommandResult { .. }));
    }
}
