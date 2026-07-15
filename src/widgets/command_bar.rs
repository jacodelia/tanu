//! Command bar widget — Vim-style `:` command input.
//!
//! Features: autocomplete (Tab), history (Up/Down).

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, UiMode};
use crate::widgets::{EventResult, Widget};

pub struct CommandBar {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    active: bool,
    input: String,
    cursor_pos: usize,
    history: Vec<String>,
    history_index: usize,
    completions: Vec<String>,
    completion_index: usize,
}

impl CommandBar {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            active: false,
            input: String::new(),
            cursor_pos: 0,
            history: Vec::new(),
            history_index: 0,
            completions: Vec::new(),
            completion_index: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_completions(&mut self, completions: Vec<String>) {
        self.completions = completions;
        self.completion_index = 0;
    }
}

impl Widget for CommandBar {
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
        false
    }

    fn is_focusable(&self) -> bool {
        false
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::ModeChanged(UiMode::Command) => {
                self.active = true;
                self.input.clear();
                self.cursor_pos = 0;
                self.history_index = self.history.len();
                self.dirty = true;
                EventResult::Consumed
            }
            Event::ModeChanged(_) => {
                if self.active {
                    self.active = false;
                    self.input.clear();
                    self.cursor_pos = 0;
                    self.dirty = true;
                }
                EventResult::NotConsumed
            }
            Event::KeyPress(key) if self.active => {
                match key.code {
                    KeyCode::Enter => {
                        if !self.input.is_empty() {
                            let cmd = format!(":{}", self.input);
                            self.history.push(self.input.clone());
                            self.input.clear();
                            self.cursor_pos = 0;
                            self.active = false;
                            self.dirty = true;
                            return EventResult::Event(Event::Command(cmd));
                        }
                    }
                    KeyCode::Escape => {
                        self.active = false;
                        self.input.clear();
                        self.cursor_pos = 0;
                        self.dirty = true;
                        return EventResult::Event(Event::ModeChanged(UiMode::Normal));
                    }
                    KeyCode::Backspace => {
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                            self.input.remove(self.cursor_pos);
                            self.dirty = true;
                        }
                    }
                    KeyCode::Up => {
                        if !self.history.is_empty() && self.history_index > 0 {
                            self.history_index -= 1;
                            self.input = self.history[self.history_index].clone();
                            self.cursor_pos = self.input.len();
                            self.dirty = true;
                        }
                    }
                    KeyCode::Down => {
                        if self.history_index < self.history.len() {
                            self.history_index += 1;
                            if self.history_index < self.history.len() {
                                self.input = self.history[self.history_index].clone();
                            } else {
                                self.input.clear();
                            }
                            self.cursor_pos = self.input.len();
                            self.dirty = true;
                        }
                    }
                    KeyCode::Tab => {
                        if !self.completions.is_empty() {
                            let idx = self.completion_index % self.completions.len();
                            self.input = self.completions[idx].clone();
                            self.cursor_pos = self.input.len();
                            self.completion_index += 1;
                            self.dirty = true;
                        }
                    }
                    KeyCode::Char(c) => {
                        self.input.insert(self.cursor_pos, c);
                        self.cursor_pos += 1;
                        self.dirty = true;
                    }
                    _ => {}
                }
                EventResult::Consumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let bg = Color::Rgb(49, 50, 68);

        if self.active {
            let prompt = format!(":{}", self.input);
            let span = Span::styled(
                prompt,
                Style::default().fg(Color::Rgb(205, 214, 244)).bg(bg),
            );
            let paragraph = Paragraph::new(span);
            frame.render_widget(paragraph, area);
        } else {
            let span = Span::styled(
                " Press : for commands  |  q quit  |  / search  |  Tab focus next  |  Space play/pause",
                Style::default()
                    .fg(Color::Rgb(108, 112, 134))
                    .bg(bg),
            );
            let paragraph = Paragraph::new(span);
            frame.render_widget(paragraph, area);
        }
    }
}
