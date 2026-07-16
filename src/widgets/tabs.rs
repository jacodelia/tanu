//! Tabs widget — displays a tab bar for view switching.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::Event;
use crate::widgets::{EventResult, Widget};

pub struct Tabs {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    tabs: Vec<String>,
    selected: usize,
}

impl Tabs {
    pub fn new(tabs: Vec<impl Into<String>>) -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            tabs: tabs.into_iter().map(|s| s.into()).collect(),
            selected: 0,
        }
    }

    /// Highlight the tab at `idx` (no-op if out of range).
    pub fn set_selected(&mut self, idx: usize) {
        if idx < self.tabs.len() && idx != self.selected {
            self.selected = idx;
            self.dirty = true;
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }
}

impl Widget for Tabs {
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

    fn handle_event(&mut self, _event: &Event) -> EventResult {
        EventResult::NotConsumed
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let active_style = Style::default()
            .fg(Color::Rgb(30, 30, 46))
            .bg(crate::theme::primary());
        let inactive_style = Style::default()
            .fg(Color::Rgb(108, 112, 134))
            .bg(Color::Rgb(49, 50, 68));

        let spans: Vec<Span> = self
            .tabs
            .iter()
            .enumerate()
            .flat_map(|(i, name)| {
                let style = if i == self.selected {
                    active_style
                } else {
                    inactive_style
                };
                vec![
                    Span::styled(" ", style),
                    Span::styled(name.as_str(), style),
                    Span::styled(" ", style),
                ]
            })
            .collect();

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }

    fn on_focus(&mut self) {
        self.focused = true;
        self.dirty = true;
    }

    fn on_blur(&mut self) {
        self.focused = false;
        self.dirty = true;
    }
}
