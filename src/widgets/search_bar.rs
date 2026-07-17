//! Search bar widget — incremental full-text search.
//!
//! Activated by `/` in normal mode. Accepts text input,
//! debounces queries, triggers FTS5 search, and highlights matches.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, UiMode};
use crate::widgets::{EventResult, Widget};

pub struct SearchBar {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    active: bool,
    query: String,
    result_count: usize,
}

impl Default for SearchBar {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchBar {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            active: false,
            query: String::new(),
            result_count: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_result_count(&mut self, count: usize) {
        self.result_count = count;
        self.dirty = true;
    }
}

impl Widget for SearchBar {
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
            Event::ModeChanged(UiMode::Search) if !self.active => {
                self.active = true;
                self.query.clear();
                self.result_count = 0;
                self.dirty = true;
                EventResult::Consumed
            }
            Event::ModeChanged(_) => {
                if self.active {
                    self.active = false;
                    self.dirty = true;
                }
                EventResult::NotConsumed
            }
            Event::KeyPress(key) if self.active => {
                match key.code {
                    KeyCode::Enter | KeyCode::Escape => {
                        self.active = false;
                        self.dirty = true;
                        return EventResult::Event(Event::ModeChanged(UiMode::Normal));
                    }
                    KeyCode::Backspace => {
                        if !self.query.is_empty() {
                            self.query.pop();
                            self.dirty = true;
                            return EventResult::Event(Event::SearchQueryChanged(
                                self.query.clone(),
                            ));
                        }
                    }
                    KeyCode::Char(c) => {
                        self.query.push(c);
                        self.dirty = true;
                        return EventResult::Event(Event::SearchQueryChanged(self.query.clone()));
                    }
                    _ => {}
                }
                EventResult::Consumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.active {
            return;
        }

        let bg = Color::Rgb(49, 50, 68);
        let fg = Color::Rgb(205, 214, 244);

        let count_str = if !self.query.is_empty() {
            format!(" [{}]", self.result_count)
        } else {
            String::new()
        };

        let text = format!("/{}{}", self.query, count_str);
        let span = Span::styled(text, Style::default().fg(fg).bg(bg));
        let paragraph = Paragraph::new(span);
        frame.render_widget(paragraph, area);
    }
}
