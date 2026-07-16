//! File explorer view — a clean, navigable single-directory browser in the
//! style of `ratatui-explorer`: a `..` parent entry, directories first, icons,
//! a selection marker, and full keyboard + mouse navigation.

use std::path::PathBuf;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
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
    /// The synthetic ".." parent entry.
    pub is_parent: bool,
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
    show_hidden: bool,
    /// Incremental search: active flag + case-insensitive name filter.
    searching: bool,
    filter: String,
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
            show_hidden: false,
            searching: false,
            filter: String::new(),
        };
        view.refresh_entries();
        view
    }

    /// Begin incremental search of the current directory.
    pub fn start_search(&mut self) {
        self.searching = true;
        self.filter.clear();
        self.refresh_entries();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.dirty = true;
    }

    /// End search, clearing the filter.
    pub fn end_search(&mut self) {
        if self.searching || !self.filter.is_empty() {
            self.searching = false;
            self.filter.clear();
            self.refresh_entries();
            self.selected_index = self.selected_index.min(self.entries.len().saturating_sub(1));
            self.dirty = true;
        }
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

    /// Enter the selected entry: descend into dirs / parent, or emit play.
    fn activate(&mut self) -> EventResult {
        if let Some(entry) = self.entries.get(self.selected_index) {
            if entry.is_dir {
                self.navigate_to(entry.path.clone());
                EventResult::Consumed
            } else {
                let path = entry.path.to_string_lossy().to_string();
                EventResult::Event(Event::Command(format!("play_file:{}", path)))
            }
        } else {
            EventResult::Consumed
        }
    }

    fn go_parent(&mut self) {
        if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
            let leaving = self.current_dir.clone();
            self.navigate_to(parent);
            // Land on the directory we came from for intuitive back-nav.
            if let Some(idx) = self.entries.iter().position(|e| e.path == leaving) {
                self.selected_index = idx;
                self.scroll_to_selection();
            }
        }
    }

    fn refresh_entries(&mut self) {
        self.entries.clear();
        let needle = self.filter.to_lowercase();

        // Synthetic parent entry (hidden while filtering).
        if needle.is_empty() {
            if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
                self.entries.push(DirEntry {
                    path: parent,
                    name: "..".to_string(),
                    is_dir: true,
                    is_parent: true,
                });
            }
        }

        if let Ok(iter) = std::fs::read_dir(&self.current_dir) {
            let mut items: Vec<DirEntry> = iter
                .filter_map(|e| e.ok())
                .map(|e| DirEntry {
                    is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
                    name: e.file_name().to_string_lossy().to_string(),
                    path: e.path(),
                    is_parent: false,
                })
                .filter(|e| self.show_hidden || !e.name.starts_with('.'))
                .filter(|e| needle.is_empty() || e.name.to_lowercase().contains(&needle))
                .collect();
            items.sort_by(|a, b| {
                a.is_dir
                    .cmp(&b.is_dir)
                    .reverse()
                    .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            self.entries.extend(items);
        }
    }

    fn visible_rows(&self) -> usize {
        self.rect.height.saturating_sub(2) as usize
    }

    fn scroll_to_selection(&mut self) {
        let visible = self.visible_rows().max(1);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible {
            self.scroll_offset = self.selected_index.saturating_sub(visible - 1);
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let max = self.entries.len() - 1;
        let cur = self.selected_index as isize;
        self.selected_index = cur.saturating_add(delta).clamp(0, max as isize) as usize;
        self.scroll_to_selection();
        self.dirty = true;
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
        let key = match event {
            Event::KeyPress(key) if self.focused => key,
            _ => return EventResult::NotConsumed,
        };

        // Incremental search: consume typing, Enter/Esc finish.
        if self.searching {
            match key.code {
                KeyCode::Escape => {
                    self.end_search();
                    return EventResult::Event(Event::ModeChanged(crate::events::UiMode::Normal));
                }
                KeyCode::Enter => {
                    // Keep the filtered results, leave search-typing mode.
                    self.searching = false;
                    return EventResult::Event(Event::ModeChanged(crate::events::UiMode::Normal));
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.refresh_entries();
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                    self.dirty = true;
                    return EventResult::Consumed;
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.refresh_entries();
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                    self.dirty = true;
                    return EventResult::Consumed;
                }
                KeyCode::Space => {
                    self.filter.push(' ');
                    self.refresh_entries();
                    self.dirty = true;
                    return EventResult::Consumed;
                }
                _ => return EventResult::Consumed,
            }
        }

        // Ctrl+H toggles hidden files.
        if key.modifiers.ctrl && matches!(key.code, KeyCode::Char('h')) {
            self.show_hidden = !self.show_hidden;
            let sel = self.selected_path().cloned();
            self.refresh_entries();
            if let Some(sel) = sel {
                if let Some(idx) = self.entries.iter().position(|e| e.path == sel) {
                    self.selected_index = idx;
                }
            }
            self.selected_index = self.selected_index.min(self.entries.len().saturating_sub(1));
            self.scroll_to_selection();
            self.dirty = true;
            return EventResult::Consumed;
        }

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => { self.move_selection(1); EventResult::Consumed }
            KeyCode::Up | KeyCode::Char('k') => { self.move_selection(-1); EventResult::Consumed }
            KeyCode::PageDown => { self.move_selection(self.visible_rows().max(1) as isize); EventResult::Consumed }
            KeyCode::PageUp => { self.move_selection(-(self.visible_rows().max(1) as isize)); EventResult::Consumed }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.activate(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => { self.go_parent(); EventResult::Consumed }
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
        }
    }

    fn handle_mouse(&mut self, _x: u16, y: u16, action: &MouseAction) -> EventResult {
        match action {
            MouseAction::Press(..) | MouseAction::RightClick(..) => {
                // Row 0 is the top border; entries start at row 1.
                if y >= 1 {
                    let row_idx = self.scroll_offset + (y - 1) as usize;
                    if row_idx < self.entries.len() {
                        self.selected_index = row_idx;
                        self.dirty = true;
                        return EventResult::Consumed;
                    }
                }
                EventResult::NotConsumed
            }
            MouseAction::DoubleClick(..) => self.activate(),
            MouseAction::ScrollUp(..) => {
                self.move_selection(-3);
                EventResult::Consumed
            }
            MouseAction::ScrollDown(..) => {
                self.move_selection(3);
                EventResult::Consumed
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

        let path_text = self.current_dir.to_string_lossy();
        let title = format!(" 🗀 {} ", path_text);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                title,
                Style::default().fg(border_color).add_modifier(Modifier::BOLD),
            ))
            .title_bottom(Span::styled(
                if self.searching || !self.filter.is_empty() {
                    format!(" /{}▏ [{}]", self.filter, self.entries.len())
                } else {
                    format!(" {}/{} {}", self.selected_index.saturating_add(1).min(self.entries.len().max(1)), self.entries.len(), if self.show_hidden { "· hidden" } else { "" })
                },
                Style::default().fg(Color::Rgb(249, 226, 175)),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible = self.visible_rows();
        self.scroll_to_selection();
        let start = self.scroll_offset;
        let end = (start + visible).min(self.entries.len());

        let sel_marker_style = Style::default().fg(Color::Rgb(137, 180, 250)).add_modifier(Modifier::BOLD);
        let sel_dir = Style::default().fg(Color::Rgb(30, 30, 46)).bg(Color::Rgb(137, 180, 250)).add_modifier(Modifier::BOLD);
        let sel_file = Style::default().fg(Color::Rgb(30, 30, 46)).bg(Color::Rgb(166, 227, 161));
        let dir_style = Style::default().fg(Color::Rgb(137, 180, 250)).add_modifier(Modifier::BOLD);
        let file_style = Style::default().fg(Color::Rgb(205, 214, 244));
        let parent_style = Style::default().fg(Color::Rgb(249, 226, 175));

        let lines: Vec<Line> = self.entries[start..end]
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let global_idx = start + i;
                let selected = global_idx == self.selected_index;

                let marker = if selected { "▶ " } else { "  " };
                let icon = if entry.is_parent {
                    "⤴ "
                } else if entry.is_dir {
                    "▸ "
                } else {
                    "♪ "
                };

                let text_style = if selected {
                    if entry.is_dir { sel_dir } else { sel_file }
                } else if entry.is_parent {
                    parent_style
                } else if entry.is_dir {
                    dir_style
                } else {
                    file_style
                };

                Line::from(vec![
                    Span::styled(marker, sel_marker_style),
                    Span::styled(format!("{}{}", icon, entry.name), text_style),
                ])
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), inner);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parent_entry_and_nav() {
        let dir = std::env::temp_dir();
        let mut b = BrowserView::new(dir.clone());
        // Non-root temp dir has a ".." entry at the top.
        assert!(b.entries.first().map(|e| e.is_parent).unwrap_or(false));
        // Down then up stays in range.
        b.focused = true;
        b.move_selection(1);
        assert!(b.selected_index <= b.entries.len().saturating_sub(1));
    }
}
