//! File explorer — an expandable directory tree (like a sidebar file tree).
//!
//! Directories expand/collapse in place; only media files are shown. Navigate
//! with the arrow keys or mouse. Activating a media file plays it and loads its
//! directory's media files (in tree order) as the play queue, so «/» step
//! through the folder.

use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, MouseAction};
use crate::widgets::{EventResult, Widget};

/// Playable media extensions (lowercase).
const MEDIA_EXTS: &[&str] = &["mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wma"];

fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| MEDIA_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
struct Row {
    path: PathBuf,
    name: String,
    is_dir: bool,
    depth: usize,
    expanded: bool,
}

pub struct BrowserView {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    root: PathBuf,
    rows: Vec<Row>,
    selected_index: usize,
    scroll_offset: usize,
    show_hidden: bool,
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
            root,
            rows: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            show_hidden: false,
            searching: false,
            filter: String::new(),
        };
        view.rebuild();
        view
    }

    pub fn current_dir(&self) -> &PathBuf {
        &self.root
    }

    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.rows.get(self.selected_index).map(|r| &r.path)
    }

    /// Move the cursor to the row for `path` (the now-playing track). Returns
    /// true if found among the visible rows.
    pub fn select_path(&mut self, path: &Path) -> bool {
        if let Some(idx) = self.rows.iter().position(|r| r.path == path) {
            if idx != self.selected_index {
                self.selected_index = idx;
                self.scroll_to_selection();
                self.dirty = true;
            }
            true
        } else {
            false
        }
    }

    /// Re-root the tree at `path` (library folder change).
    pub fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.root = path;
            self.selected_index = 0;
            self.scroll_offset = 0;
            self.rebuild();
            self.dirty = true;
        }
    }

    // ---- search -----------------------------------------------------------

    pub fn start_search(&mut self) {
        self.searching = true;
        self.filter.clear();
        self.rebuild();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.dirty = true;
    }

    pub fn end_search(&mut self) {
        if self.searching || !self.filter.is_empty() {
            self.searching = false;
            self.filter.clear();
            self.rebuild();
            self.selected_index = self.selected_index.min(self.rows.len().saturating_sub(1));
            self.dirty = true;
        }
    }

    // ---- tree building ----------------------------------------------------

    /// Read a directory's children as rows at `depth` (dirs first, media only).
    fn read_children(&self, dir: &Path, depth: usize) -> Vec<Row> {
        let needle = self.filter.to_lowercase();
        let mut items: Vec<Row> = match std::fs::read_dir(dir) {
            Ok(iter) => iter
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let path = e.path();
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let name = e.file_name().to_string_lossy().to_string();
                    if !self.show_hidden && name.starts_with('.') {
                        return None;
                    }
                    // Only directories and media files.
                    if !is_dir && !is_media(&path) {
                        return None;
                    }
                    if !needle.is_empty() && !name.to_lowercase().contains(&needle) {
                        return None;
                    }
                    Some(Row { path, name, is_dir, depth, expanded: false })
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        items.sort_by(|a, b| {
            a.is_dir
                .cmp(&b.is_dir)
                .reverse()
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        items
    }

    /// Rebuild the whole tree from the root (collapses everything).
    fn rebuild(&mut self) {
        let root = self.root.clone();
        self.rows = self.read_children(&root, 0);
    }

    /// Expand the directory at `idx` (insert children below it).
    fn expand(&mut self, idx: usize) {
        if let Some(row) = self.rows.get(idx) {
            if !row.is_dir || row.expanded {
                return;
            }
            let children = self.read_children(&row.path.clone(), row.depth + 1);
            self.rows[idx].expanded = true;
            for (i, child) in children.into_iter().enumerate() {
                self.rows.insert(idx + 1 + i, child);
            }
            self.dirty = true;
        }
    }

    /// Collapse the directory at `idx` (remove its descendants).
    fn collapse(&mut self, idx: usize) {
        let depth = match self.rows.get(idx) {
            Some(r) if r.is_dir && r.expanded => r.depth,
            _ => return,
        };
        self.rows[idx].expanded = false;
        let mut end = idx + 1;
        while end < self.rows.len() && self.rows[end].depth > depth {
            end += 1;
        }
        self.rows.drain(idx + 1..end);
        self.dirty = true;
    }

    fn toggle(&mut self, idx: usize) {
        if let Some(row) = self.rows.get(idx) {
            if row.is_dir {
                if row.expanded {
                    self.collapse(idx);
                } else {
                    self.expand(idx);
                }
            }
        }
    }

    /// Jump selection to the parent directory row of the current selection.
    fn select_parent(&mut self) {
        if let Some(row) = self.rows.get(self.selected_index) {
            if row.depth == 0 {
                return;
            }
            let depth = row.depth;
            for i in (0..self.selected_index).rev() {
                if self.rows[i].depth < depth {
                    self.selected_index = i;
                    self.scroll_to_selection();
                    self.dirty = true;
                    return;
                }
            }
        }
    }

    /// Ordered media files sharing the selected file's directory + its index.
    pub fn selected_media_queue(&self) -> Option<(Vec<String>, usize)> {
        let sel = self.rows.get(self.selected_index)?;
        if sel.is_dir {
            return None;
        }
        let dir = sel.path.parent();
        let files: Vec<&Row> = self
            .rows
            .iter()
            .filter(|r| !r.is_dir && r.path.parent() == dir)
            .collect();
        let index = files.iter().position(|r| r.path == sel.path)?;
        let paths = files.iter().map(|r| r.path.to_string_lossy().to_string()).collect();
        Some((paths, index))
    }

    /// Activate the selected row: toggle a dir, or play a file (as a dir queue).
    fn activate(&mut self) -> EventResult {
        let is_dir = match self.rows.get(self.selected_index) {
            Some(r) => r.is_dir,
            None => return EventResult::Consumed,
        };
        if is_dir {
            self.toggle(self.selected_index);
            EventResult::Consumed
        } else if let Some((paths, index)) = self.selected_media_queue() {
            EventResult::Event(Event::PlayQueue(paths, index))
        } else {
            EventResult::Consumed
        }
    }

    // ---- scrolling --------------------------------------------------------

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
        if self.rows.is_empty() {
            return;
        }
        let max = self.rows.len() - 1;
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

        // Incremental search: typing filters the tree.
        if self.searching {
            match key.code {
                KeyCode::Escape => {
                    self.end_search();
                    return EventResult::Event(Event::ModeChanged(crate::events::UiMode::Normal));
                }
                KeyCode::Enter => {
                    self.searching = false;
                    return EventResult::Event(Event::ModeChanged(crate::events::UiMode::Normal));
                }
                KeyCode::Backspace => { self.filter.pop(); self.rebuild(); self.selected_index = 0; self.scroll_offset = 0; self.dirty = true; return EventResult::Consumed; }
                KeyCode::Char(c) => { self.filter.push(c); self.rebuild(); self.selected_index = 0; self.scroll_offset = 0; self.dirty = true; return EventResult::Consumed; }
                KeyCode::Space => { self.filter.push(' '); self.rebuild(); self.dirty = true; return EventResult::Consumed; }
                _ => return EventResult::Consumed,
            }
        }

        // Ctrl+H toggles hidden files.
        if key.modifiers.ctrl && matches!(key.code, KeyCode::Char('h')) {
            self.show_hidden = !self.show_hidden;
            self.rebuild();
            self.selected_index = self.selected_index.min(self.rows.len().saturating_sub(1));
            self.scroll_to_selection();
            self.dirty = true;
            return EventResult::Consumed;
        }

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => { self.move_selection(1); EventResult::Consumed }
            KeyCode::Up | KeyCode::Char('k') => { self.move_selection(-1); EventResult::Consumed }
            KeyCode::PageDown => { self.move_selection(self.visible_rows().max(1) as isize); EventResult::Consumed }
            KeyCode::PageUp => { self.move_selection(-(self.visible_rows().max(1) as isize)); EventResult::Consumed }
            KeyCode::Enter => self.activate(),
            KeyCode::Right | KeyCode::Char('l') => {
                match self.rows.get(self.selected_index) {
                    Some(r) if r.is_dir && !r.expanded => { self.expand(self.selected_index); EventResult::Consumed }
                    Some(r) if r.is_dir => { self.move_selection(1); EventResult::Consumed }
                    _ => self.activate(),
                }
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => {
                match self.rows.get(self.selected_index) {
                    Some(r) if r.is_dir && r.expanded => { self.collapse(self.selected_index); EventResult::Consumed }
                    _ => { self.select_parent(); EventResult::Consumed }
                }
            }
            KeyCode::Home | KeyCode::Char('g') => { self.selected_index = 0; self.scroll_offset = 0; self.dirty = true; EventResult::Consumed }
            KeyCode::End | KeyCode::Char('G') => {
                if !self.rows.is_empty() {
                    self.selected_index = self.rows.len() - 1;
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
                if y >= 1 {
                    let row_idx = self.scroll_offset + (y - 1) as usize;
                    if row_idx < self.rows.len() {
                        self.selected_index = row_idx;
                        self.dirty = true;
                        return EventResult::Consumed;
                    }
                }
                EventResult::NotConsumed
            }
            MouseAction::DoubleClick(..) => self.activate(),
            MouseAction::ScrollUp(..) => { self.move_selection(-3); EventResult::Consumed }
            MouseAction::ScrollDown(..) => { self.move_selection(3); EventResult::Consumed }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let border_color = if self.focused {
            crate::theme::border_focused()
        } else {
            crate::theme::border()
        };
        let title = format!(" 🗀 {} ", self.root.to_string_lossy());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(title, Style::default().fg(border_color).add_modifier(Modifier::BOLD)))
            .title_bottom(Span::styled(
                if self.searching || !self.filter.is_empty() {
                    format!(" /{}▏ [{}]", self.filter, self.rows.len())
                } else {
                    format!(" {}/{} {}", self.selected_index.saturating_add(1).min(self.rows.len().max(1)), self.rows.len(), if self.show_hidden { "· hidden" } else { "" })
                },
                Style::default().fg(Color::Rgb(249, 226, 175)),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible = self.visible_rows();
        self.scroll_to_selection();
        let start = self.scroll_offset;
        let end = (start + visible).min(self.rows.len());

        let marker_style = Style::default().fg(crate::theme::border_focused()).add_modifier(Modifier::BOLD);
        let sel_dir = Style::default().fg(Color::Rgb(30, 30, 46)).bg(crate::theme::border_focused()).add_modifier(Modifier::BOLD);
        let sel_file = Style::default().fg(Color::Rgb(30, 30, 46)).bg(Color::Rgb(166, 227, 161));
        let dir_style = Style::default().fg(crate::theme::border_focused()).add_modifier(Modifier::BOLD);
        let file_style = Style::default().fg(Color::Rgb(205, 214, 244));

        let lines: Vec<Line> = self.rows[start..end]
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let global_idx = start + i;
                let selected = global_idx == self.selected_index;
                let marker = if selected { "▶" } else { " " };
                let indent = "  ".repeat(row.depth);
                let glyph = if row.is_dir {
                    if row.expanded { "▾ " } else { "▸ " }
                } else {
                    "♪ "
                };
                let style = if selected {
                    if row.is_dir { sel_dir } else { sel_file }
                } else if row.is_dir {
                    dir_style
                } else {
                    file_style
                };
                Line::from(vec![
                    Span::styled(format!("{} ", marker), marker_style),
                    Span::styled(format!("{}{}{}", indent, glyph, row.name), style),
                ])
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn on_focus(&mut self) { self.focused = true; self.dirty = true; }
    fn on_blur(&mut self) { self.focused = false; self.dirty = true; }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_media_filter_and_expand() {
        let dir = std::env::temp_dir().join(format!("tanu-tree-{}", std::process::id()));
        let sub = dir.join("sub");
        let _ = fs::create_dir_all(&sub);
        fs::write(dir.join("song.mp3"), b"x").unwrap();
        fs::write(dir.join("notes.txt"), b"x").unwrap();
        fs::write(sub.join("inner.flac"), b"x").unwrap();

        let mut b = BrowserView::new(dir.clone());
        // Non-media hidden: only "sub" dir + "song.mp3".
        assert!(b.rows.iter().any(|r| r.name == "song.mp3"));
        assert!(!b.rows.iter().any(|r| r.name == "notes.txt"));
        // Expand the subdir in place.
        let sub_idx = b.rows.iter().position(|r| r.is_dir).unwrap();
        b.expand(sub_idx);
        assert!(b.rows.iter().any(|r| r.name == "inner.flac" && r.depth == 1));

        let _ = fs::remove_dir_all(&dir);
    }
}
