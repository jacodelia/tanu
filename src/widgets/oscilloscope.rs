//! Oscilloscope view — real-time waveform visualizer.
//!
//! Reads a window of live samples at the current playhead from the shared
//! [`AudioViz`] buffer, so the trace is the actual audio, not a synthesized
//! wave. Falls back to a flat line when nothing is playing.

use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::audio::viz::AudioViz;
use crate::core::id::WidgetId;
use crate::events::Event;
use crate::widgets::{EventResult, Widget};

pub struct Oscilloscope {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    playing: bool,
    viz: AudioViz,
    last_frame: Instant,
}

impl Oscilloscope {
    pub fn new(viz: AudioViz) -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            playing: false,
            viz,
            last_frame: Instant::now(),
        }
    }
}

impl Widget for Oscilloscope {
    fn id(&self) -> WidgetId { self.id }
    fn rect(&self) -> Rect { self.rect }
    fn set_rect(&mut self, rect: Rect) { self.rect = rect; }
    fn is_dirty(&self) -> bool { self.dirty }
    fn mark_dirty(&mut self) { self.dirty = true; }
    fn mark_clean(&mut self) { self.dirty = false; }
    fn is_focused(&self) -> bool { self.focused }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::PlayerStateChanged(state) => {
                self.playing = state.is_playing;
                self.dirty = true;
                EventResult::Consumed
            }
            Event::Tick => {
                // Redraw ~25fps while playing so the trace follows the playhead.
                if self.last_frame.elapsed().as_millis() >= 40 && self.viz.is_active() {
                    self.last_frame = Instant::now();
                    self.dirty = true;
                }
                EventResult::NotConsumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let active = self.viz.is_active();
        let border_color = if self.focused {
            Color::Rgb(137, 180, 250)
        } else {
            Color::Rgb(69, 71, 90)
        };
        let label = if active { " Oscilloscope ▶ " } else { " Oscilloscope ⏸ " };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(label);

        let line_color = if active {
            Color::Rgb(166, 227, 161)
        } else {
            Color::Rgb(108, 112, 134)
        };

        // Real waveform window at the current playhead (flat line when idle).
        let n = 120usize;
        let wf = self.viz.waveform(n);
        let points: Vec<(f64, f64)> = if wf.is_empty() {
            (0..=1).map(|i| (i as f64, 0.0)).collect()
        } else {
            wf.iter()
                .enumerate()
                .map(|(i, &s)| (i as f64 / (wf.len() - 1).max(1) as f64, (s as f64).clamp(-1.0, 1.0)))
                .collect()
        };

        let canvas = Canvas::default()
            .block(block)
            .marker(Marker::Braille)
            .x_bounds([0.0, 1.0])
            .y_bounds([-1.0, 1.0])
            .paint(move |ctx| {
                for w in points.windows(2) {
                    ctx.draw(&CanvasLine {
                        x1: w[0].0,
                        y1: w[0].1,
                        x2: w[1].0,
                        y2: w[1].1,
                        color: line_color,
                    });
                }
            });
        frame.render_widget(canvas, area);
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
    fn test_reads_from_viz() {
        let viz = AudioViz::new();
        let scope = Oscilloscope::new(viz.clone());
        // Idle: nothing to draw.
        assert!(!scope.viz.is_active());
        // Feed audio → active.
        viz.on_play();
        for _ in 0..2000 {
            viz.push_test(0.3);
        }
        assert!(scope.viz.is_active());
    }
}
