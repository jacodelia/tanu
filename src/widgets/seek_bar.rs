//! Seek strip — shown under the visualizer. Row 0: the now-playing track name.
//! Row 1: `00:42 ██████░░░░ 03:15`, clickable/draggable to seek the playhead.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, MouseAction};
use crate::widgets::{EventResult, Widget};

pub struct SeekBar {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    position_secs: f64,
    duration_secs: f64,
    is_playing: bool,
    now_playing: String,
    /// Progress-bar hit region (widget-local): (row y, bar_start_x, bar_end_x).
    bar_region: Option<(u16, u16, u16)>,
}

impl SeekBar {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            position_secs: 0.0,
            duration_secs: 0.0,
            is_playing: false,
            now_playing: String::new(),
            bar_region: None,
        }
    }

    pub fn set_now_playing(&mut self, text: String) {
        if text != self.now_playing {
            self.now_playing = text;
            self.dirty = true;
        }
    }

    fn format_time(secs: f64) -> String {
        if secs <= 0.0 || !secs.is_finite() {
            return "--:--".to_string();
        }
        let total = secs as u64;
        format!("{:02}:{:02}", total / 60, total % 60)
    }
}

impl Default for SeekBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for SeekBar {
    fn id(&self) -> WidgetId { self.id }
    fn rect(&self) -> Rect { self.rect }
    fn set_rect(&mut self, rect: Rect) { self.rect = rect; }
    fn is_dirty(&self) -> bool { self.dirty }
    fn mark_dirty(&mut self) { self.dirty = true; }
    fn mark_clean(&mut self) { self.dirty = false; }
    fn is_focused(&self) -> bool { false }
    fn is_focusable(&self) -> bool { false }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        if let Event::PlayerStateChanged(state) = event {
            self.position_secs = state.position_secs;
            self.duration_secs = state.duration_secs;
            self.is_playing = state.is_playing;
            self.dirty = true;
            return EventResult::Consumed;
        }
        EventResult::NotConsumed
    }

    fn handle_mouse(&mut self, x: u16, y: u16, action: &MouseAction) -> EventResult {
        if !matches!(action, MouseAction::Press(..) | MouseAction::Drag(..) | MouseAction::DoubleClick(..)) {
            return EventResult::NotConsumed;
        }
        if let Some((by, start, end)) = self.bar_region {
            if y == by && x >= start && x < end && end > start && self.duration_secs > 0.0 {
                let frac = (x - start) as f32 / (end - start) as f32;
                let pos = frac as f64 * self.duration_secs;
                return EventResult::Event(Event::Seek(pos.clamp(0.0, self.duration_secs)));
            }
        }
        EventResult::NotConsumed
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.bar_region = None;
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Row 0: track name.
        let name = if self.now_playing.is_empty() { "—" } else { self.now_playing.as_str() };
        let name_line = Line::from(Span::styled(
            format!("♪ {}", name),
            Style::default().fg(Color::Rgb(166, 227, 161)).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(Paragraph::new(name_line), Rect { x: area.x, y: area.y, width: area.width, height: 1 });

        if area.height < 2 {
            return;
        }

        // Row 1: time + seekable bar.
        let pos = Self::format_time(self.position_secs);
        let dur = Self::format_time(self.duration_secs);
        let frac = if self.duration_secs > 0.0 {
            (self.position_secs / self.duration_secs).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let pre = format!(" {} ", pos);
        let post = format!(" {} ", dur);
        let label_w = pre.chars().count() as u16 + post.chars().count() as u16;
        let bar_w = area.width.saturating_sub(label_w).max(1) as usize;
        let filled = (frac * bar_w as f64) as usize;
        let empty = bar_w.saturating_sub(filled);
        let fill_color = if self.is_playing { Color::Rgb(166, 227, 161) } else { Color::Rgb(108, 112, 134) };

        let bar_start = area.x + pre.chars().count() as u16;
        let row_y = area.y + 1;
        // Local hit region for seeking.
        self.bar_region = Some((row_y - area.y, bar_start - area.x, bar_start - area.x + bar_w as u16));

        let line = Line::from(vec![
            Span::styled(pre, Style::default().fg(fill_color)),
            Span::styled("█".repeat(filled), Style::default().fg(fill_color)),
            Span::styled("░".repeat(empty), Style::default().fg(crate::theme::border())),
            Span::styled(post, Style::default().fg(Color::Rgb(108, 112, 134))),
        ]);
        frame.render_widget(Paragraph::new(line), Rect { x: area.x, y: row_y, width: area.width, height: 1 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_click_seeks() {
        let mut sb = SeekBar::new();
        sb.duration_secs = 100.0;
        sb.bar_region = Some((1, 0, 10));
        let r = sb.handle_mouse(5, 1, &MouseAction::Press(crate::events::MouseButton::Left, 5, 1));
        match r {
            EventResult::Event(Event::Seek(p)) => assert!((p - 50.0).abs() < 5.0),
            _ => panic!("expected seek event"),
        }
    }
}
