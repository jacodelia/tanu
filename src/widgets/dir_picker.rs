//! Directory picker — a centered modal folder tree with OK / Cancel buttons.
//!
//! Shows directories only (expand in place). Used by FILE → Scan Directory to
//! choose a system folder. On OK it emits `Command("pick_dir:<path>")`.

use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, MouseAction};
use crate::widgets::{EventResult, Widget};

#[derive(Debug, Clone)]
struct Row {
    path: PathBuf,
    name: String,
    depth: usize,
    expanded: bool,
    /// The synthetic ". (use this folder)" entry for the current root.
    is_root_marker: bool,
}

pub struct DirPicker {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    visible: bool,
    root: PathBuf,
    rows: Vec<Row>,
    selected_index: usize,
    scroll_offset: usize,
    /// Screen-space hit regions, rebuilt each render.
    modal_rect: Rect,
    list_top: u16,
    list_rows: u16,
    ok_region: Option<(u16, u16, u16)>,
    cancel_region: Option<(u16, u16, u16)>,
    close_region: Option<(u16, u16, u16)>,
}

impl DirPicker {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            visible: false,
            root: PathBuf::from("/"),
            rows: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            modal_rect: Rect::default(),
            list_top: 0,
            list_rows: 0,
            ok_region: None,
            cancel_region: None,
            close_region: None,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Open the picker rooted at `start` (falls back to `/`).
    pub fn show(&mut self, start: PathBuf) {
        self.root = if start.is_dir() { start } else { PathBuf::from("/") };
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.rebuild();
        self.visible = true;
        self.dirty = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.dirty = true;
    }

    fn read_dirs(dir: &Path, depth: usize) -> Vec<Row> {
        let mut items: Vec<Row> = match std::fs::read_dir(dir) {
            Ok(iter) => iter
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if !is_dir {
                        return None;
                    }
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        return None;
                    }
                    Some(Row { path: e.path(), name, depth, expanded: false, is_root_marker: false })
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        items
    }

    /// Rebuild: a "use this folder" marker for the root, then its subdirs.
    fn rebuild(&mut self) {
        let mut rows = vec![Row {
            path: self.root.clone(),
            name: ". (use this folder)".into(),
            depth: 0,
            expanded: false,
            is_root_marker: true,
        }];
        rows.extend(Self::read_dirs(&self.root.clone(), 0));
        self.rows = rows;
        self.selected_index = self.selected_index.min(self.rows.len().saturating_sub(1));
    }

    fn expand(&mut self, idx: usize) {
        if let Some(row) = self.rows.get(idx) {
            if row.is_root_marker || row.expanded {
                return;
            }
            let children = Self::read_dirs(&row.path.clone(), row.depth + 1);
            self.rows[idx].expanded = true;
            for (i, child) in children.into_iter().enumerate() {
                self.rows.insert(idx + 1 + i, child);
            }
            self.dirty = true;
        }
    }

    fn collapse(&mut self, idx: usize) {
        let depth = match self.rows.get(idx) {
            Some(r) if r.expanded => r.depth,
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

    /// Re-root at the current root's parent (go up the filesystem).
    fn go_up(&mut self) {
        if let Some(parent) = self.root.parent().map(|p| p.to_path_buf()) {
            if parent.is_dir() {
                self.root = parent;
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.rebuild();
                self.dirty = true;
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let max = self.rows.len() as isize - 1;
        self.selected_index = (self.selected_index as isize + delta).clamp(0, max) as usize;
        self.scroll_to_selection();
        self.dirty = true;
    }

    fn scroll_to_selection(&mut self) {
        let visible = (self.list_rows as usize).max(1);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible {
            self.scroll_offset = self.selected_index + 1 - visible;
        }
    }

    fn confirm(&mut self) -> EventResult {
        let path = self.rows.get(self.selected_index).map(|r| r.path.clone()).unwrap_or_else(|| self.root.clone());
        self.hide();
        EventResult::Event(Event::Command(format!("pick_dir:{}", path.to_string_lossy())))
    }
}

impl Default for DirPicker {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for DirPicker {
    fn id(&self) -> WidgetId { self.id }
    fn rect(&self) -> Rect { self.rect }
    fn set_rect(&mut self, rect: Rect) { self.rect = rect; }
    fn is_dirty(&self) -> bool { self.dirty || self.visible }
    fn mark_dirty(&mut self) { self.dirty = true; }
    fn mark_clean(&mut self) { self.dirty = false; }
    fn is_focused(&self) -> bool { self.visible }
    fn is_focusable(&self) -> bool { self.visible }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        if !self.visible {
            return EventResult::NotConsumed;
        }
        match event {
            Event::KeyPress(key) => match key.code {
                KeyCode::Escape => { self.hide(); EventResult::Consumed }
                KeyCode::Enter => self.confirm(),
                KeyCode::Down | KeyCode::Char('j') => { self.move_selection(1); EventResult::Consumed }
                KeyCode::Up | KeyCode::Char('k') => { self.move_selection(-1); EventResult::Consumed }
                KeyCode::PageDown => { self.move_selection(self.list_rows.max(1) as isize); EventResult::Consumed }
                KeyCode::PageUp => { self.move_selection(-(self.list_rows.max(1) as isize)); EventResult::Consumed }
                KeyCode::Right | KeyCode::Char('l') => { self.expand(self.selected_index); EventResult::Consumed }
                KeyCode::Left | KeyCode::Char('h') => {
                    match self.rows.get(self.selected_index) {
                        Some(r) if r.expanded => { self.collapse(self.selected_index); }
                        _ => { self.go_up(); }
                    }
                    EventResult::Consumed
                }
                KeyCode::Backspace => { self.go_up(); EventResult::Consumed }
                _ => EventResult::Consumed, // modal: swallow the rest
            },
            Event::MouseAction(action) if action.is_click() => {
                let (mx, my) = action.coords();
                if let Some((ry, x0, x1)) = self.close_region {
                    if my == ry && mx >= x0 && mx < x1 { self.hide(); return EventResult::Consumed; }
                }
                if let Some((ry, x0, x1)) = self.ok_region {
                    if my == ry && mx >= x0 && mx < x1 { return self.confirm(); }
                }
                if let Some((ry, x0, x1)) = self.cancel_region {
                    if my == ry && mx >= x0 && mx < x1 { self.hide(); return EventResult::Consumed; }
                }
                // Click on a list row.
                if my >= self.list_top && my < self.list_top + self.list_rows {
                    let idx = self.scroll_offset + (my - self.list_top) as usize;
                    if idx < self.rows.len() {
                        let double = matches!(action, MouseAction::DoubleClick(..));
                        self.selected_index = idx;
                        self.dirty = true;
                        if double {
                            // Double-click a real folder toggles expand.
                            if let Some(r) = self.rows.get(idx) {
                                if !r.is_root_marker {
                                    if r.expanded { self.collapse(idx); } else { self.expand(idx); }
                                }
                            }
                        }
                    }
                }
                EventResult::Consumed // modal swallows clicks (even outside)
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }
        let w = area.width.saturating_sub(6).min(70).max(20);
        let h = area.height.saturating_sub(4).min(24).max(8);
        let x = area.x + area.width.saturating_sub(w) / 2;
        let y = area.y + area.height.saturating_sub(h) / 2;
        let modal = Rect { x, y, width: w, height: h };
        self.modal_rect = modal;

        let border = crate::theme::border_focused();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border))
            .title(Span::styled(" Scan Folder ", Style::default().fg(crate::theme::primary()).add_modifier(Modifier::BOLD)))
            .title_top(Line::from(Span::styled("[x]", Style::default().fg(Color::Rgb(243, 139, 168)).add_modifier(Modifier::BOLD))).right_aligned())
            .title_bottom(Span::styled(
                format!(" 🗀 {} ", self.root.to_string_lossy()),
                Style::default().fg(Color::Rgb(249, 226, 175)),
            ))
            .style(Style::default().bg(Color::Rgb(30, 30, 46)));
        let inner = block.inner(modal);
        frame.render_widget(Clear, modal);
        frame.render_widget(block, modal);

        let x_end = modal.x + modal.width.saturating_sub(1);
        self.close_region = Some((modal.y, x_end.saturating_sub(3), x_end));

        if inner.width == 0 || inner.height < 2 {
            return;
        }

        // Reserve the last inner row for the OK / Cancel buttons.
        self.list_top = inner.y;
        self.list_rows = inner.height.saturating_sub(1);
        self.scroll_to_selection();

        let start = self.scroll_offset;
        let end = (start + self.list_rows as usize).min(self.rows.len());
        let sel_style = Style::default().fg(Color::Rgb(30, 30, 46)).bg(border).add_modifier(Modifier::BOLD);
        let dir_style = Style::default().fg(border);
        let root_style = Style::default().fg(Color::Rgb(166, 227, 161)).add_modifier(Modifier::BOLD);

        let lines: Vec<Line> = self.rows[start..end]
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let gi = start + i;
                let selected = gi == self.selected_index;
                let marker = if selected { "▶ " } else { "  " };
                let indent = "  ".repeat(row.depth);
                let glyph = if row.is_root_marker { "" } else if row.expanded { "▾ " } else { "▸ " };
                let style = if selected { sel_style } else if row.is_root_marker { root_style } else { dir_style };
                Line::from(vec![
                    Span::styled(marker.to_string(), Style::default().fg(border)),
                    Span::styled(format!("{}{}{}", indent, glyph, row.name), style),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), Rect { x: inner.x, y: inner.y, width: inner.width, height: self.list_rows });

        // Button row.
        let btn_y = inner.y + inner.height - 1;
        let ok = "[ OK ]";
        let cancel = "[ Cancel ]";
        let ok_w = ok.chars().count() as u16;
        let cancel_w = cancel.chars().count() as u16;
        let gap = 2u16;
        let total = ok_w + gap + cancel_w;
        let bx = inner.x + inner.width.saturating_sub(total);
        self.ok_region = Some((btn_y, bx, bx + ok_w));
        self.cancel_region = Some((btn_y, bx + ok_w + gap, bx + ok_w + gap + cancel_w));
        let btn_line = Line::from(vec![
            Span::styled(ok, Style::default().fg(Color::Rgb(30, 30, 46)).bg(Color::Rgb(166, 227, 161)).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(cancel, Style::default().fg(Color::Rgb(30, 30, 46)).bg(Color::Rgb(243, 139, 168)).add_modifier(Modifier::BOLD)),
        ]);
        frame.render_widget(
            Paragraph::new(btn_line).alignment(ratatui::layout::Alignment::Right),
            Rect { x: inner.x, y: btn_y, width: inner.width, height: 1 },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_dirs_only_and_confirm() {
        let dir = std::env::temp_dir().join(format!("tanu-pick-{}", std::process::id()));
        let sub = dir.join("music");
        let _ = fs::create_dir_all(&sub);
        fs::write(dir.join("a.txt"), b"x").unwrap();

        let mut p = DirPicker::new();
        p.show(dir.clone());
        // Row 0 = root marker; then only the "music" dir (a.txt excluded).
        assert!(p.rows[0].is_root_marker);
        assert!(p.rows.iter().any(|r| r.name == "music"));
        assert!(!p.rows.iter().any(|r| r.name == "a.txt"));

        // Confirm on the root marker → pick_dir:<dir>.
        p.selected_index = 0;
        match p.confirm() {
            EventResult::Event(Event::Command(c)) => assert_eq!(c, format!("pick_dir:{}", dir.to_string_lossy())),
            _ => panic!("expected pick_dir command"),
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
