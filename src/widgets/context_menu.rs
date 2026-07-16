use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, MouseAction};
use crate::widgets::{EventResult, Widget};

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub command: String,
}

pub struct ContextMenu {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    visible: bool,
    items: Vec<MenuItem>,
    selected_index: usize,
    /// Position where menu should appear (screen coords).
    screen_x: u16,
    screen_y: u16,
}

impl ContextMenu {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            visible: false,
            items: Vec::new(),
            selected_index: 0,
            screen_x: 0,
            screen_y: 0,
        }
    }

    pub fn show(&mut self, x: u16, y: u16, items: Vec<MenuItem>) {
        self.items = items;
        self.selected_index = 0;
        self.screen_x = x;
        self.screen_y = y;
        self.visible = true;
        self.dirty = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.dirty = true;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn selected_command(&self) -> Option<&str> {
        self.items.get(self.selected_index).map(|i| i.command.as_str())
    }

    fn menu_height(&self) -> u16 {
        (self.items.len() as u16 + 2).min(20)
    }

    fn menu_width(&self) -> u16 {
        self.items.iter()
            .map(|i| i.label.len())
            .max()
            .unwrap_or(10) as u16 + 4
    }
}

impl Widget for ContextMenu {
    fn id(&self) -> WidgetId { self.id }
    fn rect(&self) -> Rect { self.rect }
    fn set_rect(&mut self, rect: Rect) { self.rect = rect; }
    fn is_dirty(&self) -> bool { self.dirty || self.visible }
    fn mark_dirty(&mut self) { self.dirty = true; }
    fn mark_clean(&mut self) { self.dirty = false; }
    fn is_focused(&self) -> bool { self.focused }
    fn is_focusable(&self) -> bool { self.visible }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        if !self.visible {
            return EventResult::NotConsumed;
        }

        match event {
            Event::KeyPress(key) => match key.code {
                KeyCode::Escape => {
                    self.hide();
                    EventResult::Consumed
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.selected_index + 1 < self.items.len() {
                        self.selected_index += 1;
                        self.dirty = true;
                    }
                    EventResult::Consumed
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.selected_index > 0 {
                        self.selected_index -= 1;
                        self.dirty = true;
                    }
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    if self.selected_index < self.items.len() {
                        let cmd = self.items[self.selected_index].command.clone();
                        self.hide();
                        return EventResult::Event(Event::Command(cmd));
                    }
                    EventResult::Consumed
                }
                _ => EventResult::NotConsumed,
            },
            Event::MouseAction(action) => {
                // Close menu if click is outside
                if !self.visible {
                    return EventResult::NotConsumed;
                }
                if action.is_click() {
                    let (mx, my) = action.coords();
                    let inside = mx >= self.screen_x && mx < self.screen_x + self.menu_width()
                        && my >= self.screen_y && my < self.screen_y + self.menu_height();
                    if !inside {
                        self.hide();
                        return EventResult::NotConsumed;
                    }
                    // Click on an item row (row 0 is the top border): activate it.
                    let row = my.saturating_sub(self.screen_y + 1) as usize;
                    if row < self.items.len() {
                        self.selected_index = row;
                        let cmd = self.items[row].command.clone();
                        self.hide();
                        return EventResult::Event(Event::Command(cmd));
                    }
                }
                EventResult::NotConsumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn handle_mouse(&mut self, _x: u16, y: u16, action: &MouseAction) -> EventResult {
        if !self.visible {
            return EventResult::NotConsumed;
        }
        match action {
            MouseAction::Press(..) => {
                let rel_y = y.saturating_sub(1);
                let row_idx = rel_y as usize;
                if row_idx < self.items.len() {
                    self.selected_index = row_idx;
                    let cmd = self.items[self.selected_index].command.clone();
                    self.hide();
                    return EventResult::Event(Event::Command(cmd));
                }
                EventResult::Consumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible || self.items.is_empty() {
            return;
        }

        let w = self.menu_width().min(area.width.saturating_sub(self.screen_x));
        let h = self.menu_height().min(area.height.saturating_sub(self.screen_y));

        let menu_rect = Rect {
            x: self.screen_x.min(area.width.saturating_sub(w)),
            y: self.screen_y.min(area.height.saturating_sub(h)),
            width: w,
            height: h,
        };

        let border_color = Color::Rgb(137, 180, 250);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(Color::Rgb(30, 30, 46)));

        let highlight_style = Style::default().fg(Color::Rgb(30, 30, 46)).bg(Color::Rgb(137, 180, 250));
        let normal_style = Style::default().fg(Color::Rgb(205, 214, 244));

        let lines: Vec<Line> = self.items.iter()
            .enumerate()
            .map(|(i, item)| {
                let prefix = if i == self.selected_index { "▶ " } else { "  " };
                let text = format!("{}{}", prefix, item.label);
                if i == self.selected_index {
                    Line::from(Span::styled(text, highlight_style))
                } else {
                    Line::from(Span::styled(text, normal_style))
                }
            })
            .collect();

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(Clear, menu_rect);
        frame.render_widget(paragraph, menu_rect);
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
