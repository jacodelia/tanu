use std::path::PathBuf;

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, MouseAction};
use crate::widgets::{EventResult, Widget};

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

pub struct BrowserView {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    current_dir: PathBuf,
    entries: Vec<DirEntry>,
    selected_index: usize,
    scroll_offset: usize,
}

impl BrowserView {
    pub fn new(root: PathBuf) -> Self {
        let mut view = Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            current_dir: root,
            entries: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
        };
        view.refresh_entries();
        view
    }

    pub fn current_dir(&self) -> &PathBuf {
        &self.current_dir
    }

    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.entries.get(self.selected_index).map(|e| &e.path)
    }

    pub fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.current_dir = path;
            self.refresh_entries();
            self.selected_index = 0;
            self.scroll_offset = 0;
            self.dirty = true;
        }
    }

    fn refresh_entries(&mut self) {
        self.entries.clear();
        if let Ok(iter) = std::fs::read_dir(&self.current_dir) {
            let mut items: Vec<DirEntry> = iter
                .filter_map(|e| e.ok())
                .map(|e| DirEntry {
                    is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
                    name: e.file_name().to_string_lossy().to_string(),
                    path: e.path(),
                })
                .collect();
            items.sort_by(|a, b| {
                a.is_dir.cmp(&b.is_dir).reverse().then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            self.entries = items;
        }
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

impl Widget for BrowserView {
    fn id(&self) -> WidgetId { self.id }
    fn rect(&self) -> Rect { self.rect }
    fn set_rect(&mut self, rect: Rect) { self.rect = rect; }
    fn is_dirty(&self) -> bool { self.dirty }
    fn mark_dirty(&mut self) { self.dirty = true; }
    fn mark_clean(&mut self) { self.dirty = false; }
    fn is_focused(&self) -> bool { self.focused }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::KeyPress(key) if self.focused => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.selected_index + 1 < self.entries.len() {
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
                KeyCode::Enter | KeyCode::Char('l') => {
                    if let Some(entry) = self.entries.get(self.selected_index) {
                        if entry.is_dir {
                            self.navigate_to(entry.path.clone());
                        } else {
                            let path_str = entry.path.to_string_lossy().to_string();
                            return EventResult::Event(Event::Command(format!("play_file:{}", path_str)));
                        }
                    }
                    EventResult::Consumed
                }
                KeyCode::Char('h') => {
                    if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
                        self.navigate_to(parent);
                    }
                    EventResult::Consumed
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                    self.dirty = true;
                    EventResult::Consumed
                }
                KeyCode::End | KeyCode::Char('G') => {
                    if !self.entries.is_empty() {
                        self.selected_index = self.entries.len() - 1;
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
                    if row_idx < self.entries.len() {
                        self.selected_index = row_idx;
                        self.dirty = true;
                        return EventResult::Consumed;
                    }
                }
                EventResult::NotConsumed
            }
            MouseAction::DoubleClick(..) => {
                if let Some(entry) = self.entries.get(self.selected_index) {
                    if entry.is_dir {
                        self.navigate_to(entry.path.clone());
                    } else {
                        let path_str = entry.path.to_string_lossy().to_string();
                        return EventResult::Event(Event::Command(format!("play_file:{}", path_str)));
                    }
                }
                EventResult::Consumed
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
                if self.scroll_offset + visible < self.entries.len() {
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
            Color::Rgb(137, 180, 250)
        } else {
            Color::Rgb(69, 71, 90)
        };

        let dir_name = self.current_dir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        let title_text = format!(" {} [{}/{}] ", dir_name, self.selected_index.saturating_add(1).min(self.entries.len()), self.entries.len());

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
        let end = (start + visible).min(self.entries.len());

        let highlight_style = Style::default().fg(Color::Rgb(30, 30, 46)).bg(Color::Rgb(137, 180, 250));
        let dir_style = Style::default().fg(Color::Rgb(137, 180, 250));
        let file_style = Style::default().fg(Color::Rgb(205, 214, 244));

        let lines: Vec<Line> = self.entries[start..end]
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let global_idx = start + i;
                let is_selected = global_idx == self.selected_index;

                let icon = if entry.is_dir { "📁 " } else { "♪ " };
                let text = format!("{}{}", icon, entry.name);

                let style = if is_selected {
                    highlight_style
                } else if entry.is_dir {
                    dir_style
                } else {
                    file_style
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
