use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, MouseAction};
use crate::widgets::{EventResult, Widget};

pub struct PlaylistView {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    playlist_name: String,
    rows: Vec<(String, String)>,
    selected_index: usize,
    scroll_offset: usize,
}

impl PlaylistView {
    pub fn new(playlist_name: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            playlist_name: playlist_name.into(),
            rows: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
        }
    }

    pub fn set_tracks(&mut self, tracks: Vec<(String, String)>) {
        self.rows = tracks;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.dirty = true;
    }

    pub fn set_name(&mut self, name: String) {
        self.playlist_name = name;
        self.dirty = true;
    }

    pub fn selected_track_id(&self) -> Option<&str> {
        self.rows.get(self.selected_index).map(|r| r.0.as_str())
    }

    fn visible_rows(&self) -> usize {
        self.rect.height.saturating_sub(3) as usize
    }

    fn scroll_to_selection(&mut self) {
        let visible = self.visible_rows().max(1);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible {
            self.scroll_offset = self.selected_index.saturating_sub(visible - 1);
        }
    }
}

impl Widget for PlaylistView {
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
            Event::KeyPress(key) if self.focused => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.selected_index + 1 < self.rows.len() {
                        self.selected_index += 1;
                        self.dirty = true;
                    }
                    self.scroll_to_selection();
                    EventResult::Consumed
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.selected_index > 0 {
                        self.selected_index -= 1;
                        self.dirty = true;
                    }
                    self.scroll_to_selection();
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    if let Some(id) = self.selected_track_id() {
                        EventResult::Event(Event::Command(format!("play_track:{}", id)))
                    } else {
                        EventResult::Consumed
                    }
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                    self.dirty = true;
                    EventResult::Consumed
                }
                KeyCode::End | KeyCode::Char('G') => {
                    if !self.rows.is_empty() {
                        self.selected_index = self.rows.len() - 1;
                        self.scroll_to_selection();
                        self.dirty = true;
                    }
                    EventResult::Consumed
                }
                _ => EventResult::NotConsumed,
            },
            _ => EventResult::NotConsumed,
        }
    }

    fn handle_mouse(&mut self, _x: u16, y: u16, action: &MouseAction) -> EventResult {
        match action {
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
                if let Some(id) = self.selected_track_id() {
                    EventResult::Event(Event::Command(format!("play_track:{}", id)))
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

        let title_text = format!(
            " {} [{}/{}] ",
            self.playlist_name,
            self.selected_index.saturating_add(1).min(self.rows.len()),
            self.rows.len()
        );

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

        let highlight_style = Style::default()
            .fg(Color::Rgb(30, 30, 46))
            .bg(crate::theme::border_focused());
        let track_style = Style::default().fg(Color::Rgb(205, 214, 244));
        let index_style = Style::default().fg(Color::Rgb(108, 112, 134));

        let lines: Vec<Line> = self.rows[start..end]
            .iter()
            .enumerate()
            .map(|(i, (_, display))| {
                let global_idx = start + i;
                let is_selected = global_idx == self.selected_index;

                let prefix = format!(" {:>3} ", global_idx + 1);

                let mut spans = Vec::new();
                spans.push(Span::styled(prefix, index_style));
                if is_selected {
                    spans.push(Span::styled(display.clone(), highlight_style));
                } else {
                    spans.push(Span::styled(display.clone(), track_style));
                }
                Line::from(spans)
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
