//! Status bar widget — shows playback state, current mode, and track info.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, UiMode};
use crate::widgets::{EventResult, Widget};

pub struct StatusBar {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    mode: UiMode,
    is_playing: bool,
    volume: f32,
    shuffle: bool,
    now_playing: String,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            mode: UiMode::Normal,
            is_playing: false,
            volume: 0.8,
            shuffle: false,
            now_playing: String::new(),
        }
    }

    /// Set the "artist / title" text shown for the current track.
    pub fn set_now_playing(&mut self, text: impl Into<String>) {
        self.now_playing = text.into();
        self.dirty = true;
    }

    fn mode_name(&self) -> &str {
        match self.mode {
            UiMode::Normal => "NORMAL",
            UiMode::Insert => "INSERT",
            UiMode::Command => "COMMAND",
            UiMode::Visual => "VISUAL",
            UiMode::Library => "LIBRARY",
            UiMode::Browser => "BROWSER",
            UiMode::Search => "SEARCH",
            UiMode::Queue => "QUEUE",
        }
    }
}

impl Widget for StatusBar {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn rect(&self) -> Rect {
        self.rect
    }

    fn set_rect(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn mark_clean(&mut self) {
        self.dirty = false;
    }

    fn is_focused(&self) -> bool {
        self.focused
    }

    fn is_focusable(&self) -> bool {
        false
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::ModeChanged(mode) => {
                self.mode = *mode;
                self.dirty = true;
                EventResult::Consumed
            }
            Event::PlayerStateChanged(state) => {
                self.is_playing = state.is_playing;
                self.volume = state.volume;
                self.shuffle = state.shuffle;
                self.dirty = true;
                EventResult::Consumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let mode_style = match self.mode {
            UiMode::Normal => Style::default().fg(Color::Black).bg(Color::LightCyan),
            UiMode::Insert => Style::default().fg(Color::Black).bg(Color::LightGreen),
            UiMode::Command => Style::default().fg(Color::Black).bg(Color::LightYellow),
            UiMode::Visual => Style::default().fg(Color::Black).bg(Color::LightMagenta),
            _ => Style::default().fg(Color::Black).bg(Color::Cyan),
        };

        let play_icon = if self.is_playing { "[>]" } else { "[||]" };
        let shuffle_icon = if self.shuffle { "[S]" } else { "" };
        let vol_text = format!("vol:{}% ", (self.volume * 100.0) as u8);
        let now = if self.now_playing.is_empty() {
            "Tanu — Terminal Audio Navigator & Utility".to_string()
        } else {
            format!("♪ {}", self.now_playing)
        };

        let line = Line::from(vec![
            Span::styled(format!(" {} ", self.mode_name()), mode_style),
            Span::raw(" "),
            Span::raw(play_icon),
            Span::raw(" "),
            Span::raw(shuffle_icon),
            Span::raw(" "),
            Span::raw(vol_text),
            Span::styled(now, Style::default().fg(Color::Rgb(166, 227, 161))),
        ]);

        let paragraph = Paragraph::new(line)
            .style(Style::default().fg(Color::White).bg(Color::Rgb(30, 30, 46)));
        frame.render_widget(paragraph, area);
    }
}
