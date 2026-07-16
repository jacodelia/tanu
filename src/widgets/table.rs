//! Table widget — displays a scrollable list of items with
//! virtual scrolling (only renders visible rows) and selection.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, MouseAction};
use crate::widgets::{EventResult, Widget};

/// A row in the table.
#[derive(Debug, Clone)]
pub struct TableRow {
    pub id: String,
    pub display: String,
}

pub struct TableWidget {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    title: String,
    rows: Vec<TableRow>,
    selected_index: usize,
    scroll_offset: usize,
    visual_mode: bool,
    visual_start: usize,
}

impl TableWidget {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            title: title.into(),
            rows: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            visual_mode: false,
            visual_start: 0,
        }
    }

    /// Replace all rows.
    pub fn set_rows(&mut self, rows: Vec<TableRow>) {
        self.rows = rows;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.dirty = true;
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn selected_row(&self) -> Option<&TableRow> {
        self.rows.get(self.selected_index)
    }

    /// Move selection down.
    fn move_down(&mut self) {
        if self.selected_index + 1 < self.rows.len() {
            self.selected_index += 1;
            self.dirty = true;
        }
    }

    /// Move selection up.
    fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.dirty = true;
        }
    }

    /// Page down.
    fn page_down(&mut self) {
        let visible = self.visible_rows();
        let skip = visible.max(1);
        self.selected_index = (self.selected_index + skip).min(self.rows.len().saturating_sub(1));
        self.dirty = true;
    }

    /// Page up.
    fn page_up(&mut self) {
        let visible = self.visible_rows();
        let skip = visible.max(1);
        self.selected_index = self.selected_index.saturating_sub(skip);
        self.dirty = true;
    }

    fn visible_rows(&self) -> usize {
        self.rect.height.saturating_sub(3) as usize // border + padding
    }

    /// Ensure the selected row is visible.
    fn scroll_to_selection(&mut self) {
        let visible = self.visible_rows().max(1);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible {
            self.scroll_offset = self.selected_index.saturating_sub(visible - 1);
        }
    }

    fn visual_range(&self) -> (usize, usize) {
        let start = self.visual_start.min(self.selected_index);
        let end = self.visual_start.max(self.selected_index);
        (start, end)
    }

    fn is_in_visual_range(&self, index: usize) -> bool {
        if !self.visual_mode {
            return false;
        }
        let (start, end) = self.visual_range();
        index >= start && index <= end
    }

    fn yank_selection(&mut self) -> EventResult {
        let (start, end) = self.visual_range();
        let ids: Vec<String> = self.rows[start..=end.min(self.rows.len().saturating_sub(1))]
            .iter()
            .map(|r| r.id.clone())
            .collect();
        self.visual_mode = false;
        self.dirty = true;
        let paths = ids.join(",");
        EventResult::Event(Event::Command(format!("yank:{}", paths)))
    }
}

impl Widget for TableWidget {
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

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::KeyPress(key) if self.focused => {
                if self.visual_mode {
                    match key.code {
                        KeyCode::Escape => {
                            self.visual_mode = false;
                            self.dirty = true;
                            EventResult::Consumed
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            self.selected_index = (self.selected_index + 1).min(self.rows.len().saturating_sub(1));
                            self.scroll_to_selection();
                            EventResult::Consumed
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            self.selected_index = self.selected_index.saturating_sub(1);
                            self.scroll_to_selection();
                            EventResult::Consumed
                        }
                        KeyCode::Char('y') => {
                            self.yank_selection()
                        }
                        KeyCode::Char('V') if key.modifiers.shift => {
                            self.visual_mode = false;
                            self.dirty = true;
                            EventResult::Consumed
                        }
                        _ => EventResult::NotConsumed,
                    }
                } else {
                    match key.code {
                        KeyCode::Down | KeyCode::Char('j') => {
                            self.move_down();
                            self.scroll_to_selection();
                            EventResult::Consumed
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            self.move_up();
                            self.scroll_to_selection();
                            EventResult::Consumed
                        }
                        KeyCode::PageDown => {
                            self.page_down();
                            self.scroll_to_selection();
                            EventResult::Consumed
                        }
                        KeyCode::PageUp => {
                            self.page_up();
                            self.scroll_to_selection();
                            EventResult::Consumed
                        }
                        KeyCode::Home | KeyCode::Char('g') => {
                            if key.modifiers.shift {
                                // Shift+G: visual line mode with full range
                                self.visual_mode = true;
                                self.visual_start = self.selected_index;
                                self.selected_index = self.rows.len().saturating_sub(1);
                                self.scroll_to_selection();
                                EventResult::Consumed
                            } else {
                                self.selected_index = 0;
                                self.scroll_offset = 0;
                                self.dirty = true;
                                EventResult::Consumed
                            }
                        }
                        KeyCode::End | KeyCode::Char('G') => {
                            if key.modifiers.shift {
                                self.visual_mode = true;
                                self.visual_start = self.selected_index;
                                if !self.rows.is_empty() {
                                    self.selected_index = self.rows.len() - 1;
                                }
                                self.scroll_to_selection();
                                EventResult::Consumed
                            } else {
                                if !self.rows.is_empty() {
                                    self.selected_index = self.rows.len() - 1;
                                    self.scroll_to_selection();
                                    self.dirty = true;
                                }
                                EventResult::Consumed
                            }
                        }
                        KeyCode::Char('v') => {
                            if key.modifiers.shift {
                                // Shift+v: line-wise visual
                                self.visual_mode = true;
                                self.visual_start = self.selected_index;
                                self.dirty = true;
                                EventResult::Consumed
                            } else {
                                self.visual_mode = true;
                                self.visual_start = self.selected_index;
                                self.dirty = true;
                                EventResult::Consumed
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(row) = self.selected_row() {
                                EventResult::Event(Event::Command(format!("select_track:{}", row.id)))
                            } else {
                                EventResult::Consumed
                            }
                        }
                        _ => EventResult::NotConsumed,
                    }
                }
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn handle_mouse(&mut self, _x: u16, y: u16, action: &MouseAction) -> EventResult {
        match action {
            MouseAction::Drag(..) => {
                let header_height = 1u16;
                if y >= header_height {
                    let rel_y = y - header_height;
                    let row_idx = self.scroll_offset + rel_y as usize;
                    if row_idx < self.rows.len() && row_idx != self.selected_index {
                        let from = self.selected_index;
                        self.selected_index = row_idx;
                        self.dirty = true;
                        self.scroll_to_selection();
                        return EventResult::Event(Event::Command(format!(
                            "move_item:{}:{}", from, row_idx
                        )));
                    }
                }
                EventResult::NotConsumed
            }
            MouseAction::Press(..) | MouseAction::RightClick(..) => {
                let header_height = 1u16;
                if y >= header_height {
                    let rel_y = y - header_height;
                    let row_idx = self.scroll_offset + rel_y as usize;
                    if row_idx < self.rows.len() {
                        self.selected_index = row_idx;
                        self.dirty = true;
                        return EventResult::Consumed;
                    }
                }
                EventResult::NotConsumed
            }
            MouseAction::DoubleClick(..) => {
                if let Some(row) = self.selected_row() {
                    EventResult::Event(Event::Command(format!("play_track:{}", row.id)))
                } else {
                    EventResult::Consumed
                }
            }
            MouseAction::ScrollUp(..) => {
                if self.scroll_offset > 0 {
                    self.scroll_offset = self.scroll_offset.saturating_sub(3);
                    self.dirty = true;
                    EventResult::Consumed
                } else {
                    EventResult::NotConsumed
                }
            }
            MouseAction::ScrollDown(..) => {
                let visible = self.visible_rows().max(1);
                if self.scroll_offset + visible < self.rows.len() {
                    self.scroll_offset += 3;
                    self.dirty = true;
                    EventResult::Consumed
                } else {
                    EventResult::NotConsumed
                }
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let border_color = if self.focused {
            crate::theme::border_focused()
        } else {
            crate::theme::border()
        };

        let highlight_style = Style::default().fg(Color::Rgb(30, 30, 46)).bg(crate::theme::border_focused());
        let visual_style = Style::default().fg(Color::Rgb(205, 214, 244)).bg(crate::theme::border());
        let normal_style = Style::default().fg(Color::Rgb(205, 214, 244));

        let title_text = if self.visual_mode {
            let (s, e) = self.visual_range();
            format!(" {} [{}-{}/{}] VISUAL ", self.title, s + 1, e + 1, self.rows.len())
        } else {
            format!(
                " {} [{}/{}] ",
                self.title,
                self.selected_index.saturating_add(1).min(self.rows.len()),
                self.rows.len()
            )
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                title_text,
                Style::default().fg(border_color).bg(Color::Rgb(30, 30, 46)),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible = self.visible_rows();
        self.scroll_to_selection();

        let start = self.scroll_offset;
        let end = (start + visible).min(self.rows.len());

        let lines: Vec<Line> = self.rows[start..end]
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let global_idx = start + i;
                let in_visual = self.is_in_visual_range(global_idx);
                let is_cursor = global_idx == self.selected_index;

                let prefix = if is_cursor {
                    "▶ "
                } else {
                    "  "
                };
                let text = format!("{}{}", prefix, row.display);

                let style = if is_cursor {
                    highlight_style
                } else if in_visual {
                    visual_style
                } else {
                    normal_style
                };

                Line::from(Span::styled(text, style))
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
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
