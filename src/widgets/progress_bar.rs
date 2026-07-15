//! Progress bar widget — shows playback position.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::Event;
use crate::widgets::{EventResult, Widget};

pub struct ProgressBar {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    position_secs: f64,
    duration_secs: f64,
    is_playing: bool,
}

impl ProgressBar {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            position_secs: 0.0,
            duration_secs: 0.0,
            is_playing: false,
        }
    }

    fn format_time(secs: f64) -> String {
        if secs <= 0.0 || !secs.is_finite() {
            return "--:--".to_string();
        }
        let total = secs as u64;
        let minutes = total / 60;
        let seconds = total % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }
}

impl Widget for ProgressBar {
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
        false
    }

    fn is_focusable(&self) -> bool {
        false
    }

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

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let pos_text = Self::format_time(self.position_secs);
        let dur_text = Self::format_time(self.duration_secs);

        let fraction = if self.duration_secs > 0.0 {
            (self.position_secs / self.duration_secs).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Build a progress bar using block characters
        let bar_width = area.width.saturating_sub(14) as usize; // reserve space for timestamps
        let bar_width = bar_width.max(10);
        let filled = (fraction * bar_width as f64) as usize;
        let empty = bar_width.saturating_sub(filled);

        let filled_color = if self.is_playing {
            Color::Rgb(166, 227, 161)
        } else {
            Color::Rgb(108, 112, 134)
        };
        let empty_color = Color::Rgb(69, 71, 90);

        let bar = format!(
            "{}{}",
            "█".repeat(filled),
            "░".repeat(empty)
        );

        let line = Line::from(vec![
            Span::styled(
                format!(" {} ", pos_text),
                Style::default().fg(filled_color),
            ),
            Span::styled(bar, Style::default().fg(filled_color)),
            Span::styled(
                format!(" {} ", dur_text),
                Style::default().fg(empty_color),
            ),
        ]);

        let paragraph = Paragraph::new(line)
            .style(Style::default().bg(Color::Rgb(30, 30, 46)));
        frame.render_widget(paragraph, area);
    }
}
