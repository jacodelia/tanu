use std::collections::HashSet;

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::database::Database;
use crate::events::{Event, KeyCode, MouseAction};
use crate::widgets::{EventResult, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Artist,
    Album,
    Track,
}

#[derive(Debug, Clone)]
struct TreeNode {
    key: String,
    label: String,
    kind: NodeKind,
    indent: u16,
    has_children: bool,
}

pub struct LibraryView {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    db: Option<Database>,
    rows: Vec<TreeNode>,
    expanded: HashSet<String>,
    selected_index: usize,
    scroll_offset: usize,
}

impl LibraryView {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            db: None,
            rows: Vec::new(),
            expanded: HashSet::new(),
            selected_index: 0,
            scroll_offset: 0,
        }
    }

    pub fn set_database(&mut self, db: Database) {
        self.db = Some(db);
    }

    pub fn refresh(&mut self) {
        self.rows = self.load_tree();
        self.dirty = true;
    }

    fn load_tree(&self) -> Vec<TreeNode> {
        let mut rows = Vec::new();
        let db = match &self.db {
            Some(db) => db,
            None => return rows,
        };

        let artists: Vec<(String, String)> = match db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT a.id, a.name FROM artists a
                 WHERE EXISTS (SELECT 1 FROM tracks t WHERE t.artist_id = a.id)
                 ORDER BY a.name"
            )?;
            let items: Vec<(String, String)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(items)
        }) {
            Ok(items) => items,
            Err(_) => return rows,
        };

        for (artist_id, artist_name) in &artists {
            let expanded = self.expanded.contains(artist_id);
            let node = TreeNode {
                key: artist_id.clone(),
                label: artist_name.clone(),
                kind: NodeKind::Artist,
                indent: 0,
                has_children: true,
            };
            rows.push(node);

            if expanded {
                let albums: Vec<(String, String)> = match db.with_connection(|conn| {
                    let mut stmt = conn.prepare(
                        "SELECT al.id, al.title FROM albums al
                         WHERE al.artist_id = ?1
                         ORDER BY al.year, al.title"
                    )?;
                    let items: Vec<(String, String)> = stmt
                        .query_map([artist_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(items)
                }) {
                    Ok(items) => items,
                    Err(_) => continue,
                };

                for (album_id, album_title) in &albums {
                    let album_expanded = self.expanded.contains(album_id);
                    let node = TreeNode {
                        key: album_id.clone(),
                        label: album_title.clone(),
                        kind: NodeKind::Album,
                        indent: 2,
                        has_children: true,
                    };
                    rows.push(node);

                    if album_expanded {
                        let tracks: Vec<(String, String)> = match db.with_connection(|conn| {
                            let mut stmt = conn.prepare(
                                "SELECT t.id, t.title FROM tracks t
                                 WHERE t.album_id = ?1
                                 ORDER BY t.track_number, t.title"
                            )?;
                            let items: Vec<(String, String)> = stmt
                                .query_map([album_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                                .filter_map(|r| r.ok())
                                .collect();
                            Ok(items)
                        }) {
                            Ok(items) => items,
                            Err(_) => continue,
                        };

                        for (track_id, track_title) in &tracks {
                            rows.push(TreeNode {
                                key: track_id.clone(),
                                label: track_title.clone(),
                                kind: NodeKind::Track,
                                indent: 4,
                                has_children: false,
                            });
                        }
                    }
                }
            }
        }

        rows
    }

    fn toggle_expand(&mut self) {
        if let Some(row) = self.rows.get(self.selected_index) {
            let key = row.key.clone();
            if self.expanded.contains(&key) {
                self.expanded.remove(&key);
            } else {
                self.expanded.insert(key);
            }
            self.refresh();
        }
    }

    fn selected_key(&self) -> Option<&str> {
        self.rows.get(self.selected_index).map(|r| r.key.as_str())
    }

    fn selected_kind(&self) -> Option<NodeKind> {
        self.rows.get(self.selected_index).map(|r| r.kind)
    }

    fn move_down(&mut self) {
        if self.selected_index + 1 < self.rows.len() {
            self.selected_index += 1;
            self.dirty = true;
        }
    }

    fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.dirty = true;
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

impl Widget for LibraryView {
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
                    self.move_down();
                    self.scroll_to_selection();
                    EventResult::Consumed
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_up();
                    self.scroll_to_selection();
                    EventResult::Consumed
                }
                KeyCode::Enter | KeyCode::Char('l') => {
                    match self.selected_kind() {
                        Some(NodeKind::Artist) | Some(NodeKind::Album) => {
                            self.toggle_expand();
                        }
                        Some(NodeKind::Track) => {
                            if let Some(key) = self.selected_key() {
                                return EventResult::Event(Event::Command(format!("play_track:{}", key)));
                            }
                        }
                        None => {}
                    }
                    EventResult::Consumed
                }
                KeyCode::Char('h') => {
                    if let Some(row) = self.rows.get(self.selected_index) {
                        match row.kind {
                            NodeKind::Artist | NodeKind::Album => {
                                if self.expanded.contains(&row.key) {
                                    self.expanded.remove(&row.key);
                                    self.refresh();
                                }
                            }
                            NodeKind::Track => {
                                self.selected_index = self.rows.iter()
                                    .rposition(|r| r.kind == NodeKind::Album)
                                    .unwrap_or(0);
                                self.dirty = true;
                            }
                        }
                    }
                    EventResult::Consumed
                }
                KeyCode::PageDown => {
                    let visible = self.visible_rows().max(1);
                    self.selected_index = (self.selected_index + visible).min(self.rows.len().saturating_sub(1));
                    self.scroll_to_selection();
                    self.dirty = true;
                    EventResult::Consumed
                }
                KeyCode::PageUp => {
                    let visible = self.visible_rows().max(1);
                    self.selected_index = self.selected_index.saturating_sub(visible);
                    self.scroll_to_selection();
                    self.dirty = true;
                    EventResult::Consumed
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
                self.toggle_expand();
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
            Color::Rgb(137, 180, 250)
        } else {
            Color::Rgb(69, 71, 90)
        };

        let title_text = format!(
            " Library [{}/{}] ",
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

        let highlight_style = Style::default().fg(Color::Rgb(30, 30, 46)).bg(Color::Rgb(137, 180, 250));
        let artist_style = Style::default().fg(Color::Rgb(245, 194, 231));
        let album_style = Style::default().fg(Color::Rgb(166, 227, 161));
        let track_style = Style::default().fg(Color::Rgb(205, 214, 244));

        let lines: Vec<Line> = self.rows[start..end]
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let global_idx = start + i;
                let is_selected = global_idx == self.selected_index;

                let expand_icon = if row.has_children {
                    if self.expanded.contains(&row.key) { "▼ " } else { "▶ " }
                } else {
                    "  "
                };

                let indent_str = " ".repeat(row.indent as usize);
                let prefix = if is_selected { "" } else { " " };
                let text = format!("{}{}{}{} {}", prefix, indent_str, expand_icon, prefix, row.label);

                let style = if is_selected {
                    highlight_style
                } else {
                    match row.kind {
                        NodeKind::Artist => artist_style,
                        NodeKind::Album => album_style,
                        NodeKind::Track => track_style,
                    }
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
