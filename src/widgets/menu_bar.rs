//! Menu bar widget — top row with FILE / EDIT / ABOUT menus plus
//! clickable view tabs (Library / Browser / Playlist / Queue).
//!
//! Menu labels open a dropdown (via a `menu:<name>:<x>` command that the
//! app turns into a context menu). View labels switch the main panel.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, MouseAction};
use crate::widgets::{EventResult, Widget};

/// A clickable region in the bar and the command it emits.
struct Segment {
    start: u16,
    end: u16, // exclusive
    command: String,
}

pub struct MenuBar {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    /// Hit-test regions, rebuilt every render.
    segments: Vec<Segment>,
}

const MENUS: [(&str, &str); 3] = [
    ("FILE", "menu:file"),
    ("EDIT", "menu:edit"),
    ("ABOUT", "about"),
];

impl MenuBar {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            segments: Vec::new(),
        }
    }

    /// Build spans and record hit regions. Returns the assembled line.
    fn build(&mut self) -> Line<'static> {
        self.segments.clear();
        let mut spans: Vec<Span> = Vec::new();
        let mut x: u16 = 0;

        let menu_style = Style::default().fg(Color::Rgb(205, 214, 244)).bg(Color::Rgb(49, 50, 68));
        let brand_style = Style::default().fg(Color::Rgb(203, 166, 247)).bg(Color::Rgb(49, 50, 68));

        let push = |spans: &mut Vec<Span>, segments: &mut Vec<Segment>, x: &mut u16, text: String, style: Style, command: String| {
            let start = *x;
            let w = text.chars().count() as u16;
            spans.push(Span::styled(text, style));
            *x += w;
            segments.push(Segment { start, end: *x, command });
        };

        for (label, cmd) in MENUS.iter() {
            push(&mut spans, &mut self.segments, &mut x, format!(" {} ", label), menu_style, cmd.to_string());
            spans.push(Span::raw(" "));
            x += 1;
        }

        spans.push(Span::styled("   ♪ tanu", brand_style));

        Line::from(spans)
    }

    fn command_at(&self, x: u16) -> Option<String> {
        self.segments
            .iter()
            .find(|s| x >= s.start && x < s.end)
            .map(|s| {
                // Menu labels need the click column so the app can place the dropdown.
                if let Some(rest) = s.command.strip_prefix("menu:") {
                    format!("menu:{}:{}", rest, s.start)
                } else {
                    s.command.clone()
                }
            })
    }
}

impl Default for MenuBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for MenuBar {
    fn id(&self) -> WidgetId { self.id }
    fn rect(&self) -> Rect { self.rect }
    fn set_rect(&mut self, rect: Rect) { self.rect = rect; }
    fn is_dirty(&self) -> bool { self.dirty }
    fn mark_dirty(&mut self) { self.dirty = true; }
    fn mark_clean(&mut self) { self.dirty = false; }
    fn is_focused(&self) -> bool { self.focused }
    fn is_focusable(&self) -> bool { false }

    fn handle_mouse(&mut self, x: u16, _y: u16, action: &MouseAction) -> EventResult {
        match action {
            MouseAction::Press(..) | MouseAction::DoubleClick(..) => {
                if let Some(cmd) = self.command_at(x) {
                    return EventResult::Event(Event::Command(cmd));
                }
                EventResult::NotConsumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let line = self.build();
        let bar_bg = Style::default().bg(Color::Rgb(49, 50, 68));
        let paragraph = Paragraph::new(line).style(bar_bg);
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_click_maps_to_command() {
        let mut bar = MenuBar::new();
        // Build regions (render path) with a dummy area via build().
        let _ = bar.build();
        // FILE label starts at column 0 (" FILE ").
        let cmd = bar.command_at(1).unwrap();
        assert_eq!(cmd, "menu:file:0");
    }

    #[test]
    fn test_about_segment() {
        let mut bar = MenuBar::new();
        let _ = bar.build();
        let seg = bar.segments.iter().find(|s| s.command == "about").unwrap();
        let mid = (seg.start + seg.end) / 2;
        assert_eq!(bar.command_at(mid).unwrap(), "about");
    }
}
