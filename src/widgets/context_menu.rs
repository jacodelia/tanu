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
    /// When set, the menu is centered on screen and shows this title.
    modal_title: Option<String>,
    /// Screen coords of the modal `[x]` close button: (row, x_start, x_end).
    close_region: Option<(u16, u16, u16)>,
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
            modal_title: None,
            close_region: None,
        }
    }

    pub fn show(&mut self, x: u16, y: u16, items: Vec<MenuItem>) {
        self.items = items;
        self.selected_index = 0;
        self.screen_x = x;
        self.screen_y = y;
        self.visible = true;
        self.modal_title = None;
        self.dirty = true;
    }

    /// Show a centered modal menu with a title bar.
    pub fn show_modal(&mut self, title: &str, items: Vec<MenuItem>) {
        self.items = items;
        self.selected_index = 0;
        self.visible = true;
        self.modal_title = Some(title.to_string());
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
        let title = if self.modal_title.is_some() { 1 } else { 0 };
        (self.items.len() as u16 + 2 + title).min(22)
    }

    /// Row offset (within the menu) where item 0 begins: 1 for the top border,
    /// +1 more when a modal title bar is shown.
    fn items_top_offset(&self) -> u16 {
        1 + if self.modal_title.is_some() { 1 } else { 0 }
    }

    fn menu_width(&self) -> u16 {
        let items = self.items.iter().map(|i| i.label.chars().count()).max().unwrap_or(10);
        let title = self.modal_title.as_ref().map(|t| t.chars().count()).unwrap_or(0);
        // +2 swatch, +4 borders/prefix.
        items.max(title) as u16 + 6
    }
}

/// If the command is `text_color:#hex`, return the swatch color.
fn item_color(command: &str) -> Option<Color> {
    command.strip_prefix("text_color:")
        .and_then(crate::theme::parse_color)
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
                    // Modal [x] close button.
                    if let Some((ry, x0, x1)) = self.close_region {
                        if my == ry && mx >= x0 && mx < x1 {
                            self.hide();
                            return EventResult::Consumed;
                        }
                    }
                    let inside = mx >= self.screen_x && mx < self.screen_x + self.menu_width()
                        && my >= self.screen_y && my < self.screen_y + self.menu_height();
                    if !inside {
                        self.hide();
                        return EventResult::NotConsumed;
                    }
                    // Click on an item row (top border + optional title first).
                    let row = my.saturating_sub(self.screen_y + self.items_top_offset()) as usize;
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

        let w = self.menu_width().min(area.width);
        let h = self.menu_height().min(area.height);

        // Modal: centered; otherwise anchored at the click position. Store the
        // resolved top-left so mouse hit-testing (next frame) matches the draw.
        if self.modal_title.is_some() {
            self.screen_x = area.x + area.width.saturating_sub(w) / 2;
            self.screen_y = area.y + area.height.saturating_sub(h) / 2;
        }
        let menu_rect = Rect {
            x: self.screen_x.min(area.width.saturating_sub(w)),
            y: self.screen_y.min(area.height.saturating_sub(h)),
            width: w,
            height: h,
        };

        let border_color = crate::theme::border_focused();
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(Color::Rgb(30, 30, 46)));
        self.close_region = None;
        if let Some(ref title) = self.modal_title {
            block = block.title(Span::styled(
                format!(" {} ", title),
                Style::default().fg(crate::theme::primary()).add_modifier(ratatui::style::Modifier::BOLD),
            ));
            block = block.title_top(Line::from(Span::styled(
                "[x]",
                Style::default().fg(Color::Rgb(243, 139, 168)).add_modifier(ratatui::style::Modifier::BOLD),
            )).right_aligned());
            let x_end = menu_rect.x + menu_rect.width.saturating_sub(1);
            self.close_region = Some((menu_rect.y, x_end.saturating_sub(3), x_end));
        }

        let highlight_bg = crate::theme::border_focused();
        let normal_fg = Color::Rgb(205, 214, 244);

        let mut lines: Vec<Line> = Vec::with_capacity(self.items.len() + 1);
        if self.modal_title.is_some() {
            lines.push(Line::from("")); // spacer under the title bar
        }
        for (i, item) in self.items.iter().enumerate() {
            let selected = i == self.selected_index;
            let prefix = if selected { "▶ " } else { "  " };
            let mut spans = vec![Span::raw(prefix.to_string())];
            // Colored swatch for palette items (text_color:#hex).
            if let Some(c) = item_color(&item.command) {
                spans.push(Span::styled("● ", Style::default().fg(c)));
            }
            let label_style = if selected {
                Style::default().fg(Color::Rgb(30, 30, 46)).bg(highlight_bg).add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                Style::default().fg(item_color(&item.command).unwrap_or(normal_fg))
            };
            spans.push(Span::styled(item.label.clone(), label_style));
            lines.push(Line::from(spans));
        }

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
