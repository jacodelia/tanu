//! Visualizer panel with two selectable views:
//!   - **Wave**: real-time oscilloscope (samples at the playhead).
//!   - **Spec**: 16-band spectrum analyzer (Goertzel).
//!
//! Switch with the `WAVE`/`SPEC` tabs (click) or the `m` key / Tab when focused.

use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::audio::viz::AudioViz;
use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, MouseAction};
use crate::widgets::{EventResult, Widget};

const SPEC_BANDS: usize = 16;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Wave,
    Spec,
}

pub struct Oscilloscope {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    viz: AudioViz,
    mode: Mode,
    levels: [f32; SPEC_BANDS],
    freqs: [f64; SPEC_BANDS],
    last_frame: Instant,
    /// Local x ranges of the WAVE / SPEC tabs (rebuilt on render).
    tab_wave: (u16, u16),
    tab_spec: (u16, u16),
    tab_row: u16,
}

impl Oscilloscope {
    pub fn new(viz: AudioViz) -> Self {
        let (lo, hi) = (55.0_f64, 2400.0_f64);
        let mut freqs = [0.0; SPEC_BANDS];
        for (i, f) in freqs.iter_mut().enumerate() {
            let t = i as f64 / (SPEC_BANDS - 1) as f64;
            *f = lo * (hi / lo).powf(t);
        }
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            viz,
            mode: Mode::Wave,
            levels: [0.0; SPEC_BANDS],
            freqs,
            last_frame: Instant::now(),
            tab_wave: (0, 0),
            tab_spec: (0, 0),
            tab_row: 0,
        }
    }

    fn analyze(&mut self) {
        let samples = self.viz.raw_window();
        let rate = self.viz.rate();
        if samples.len() < 32 || rate <= 0.0 {
            for l in self.levels.iter_mut() {
                *l *= 0.8;
            }
            return;
        }
        for i in 0..SPEC_BANDS {
            let mag = goertzel(&samples, self.freqs[i], rate);
            let level = (mag * 6.0).sqrt().min(1.0);
            let prev = self.levels[i];
            self.levels[i] = if level > prev { level } else { prev * 0.82 + level * 0.18 };
        }
    }

    fn render_wave(&self, frame: &mut Frame, area: Rect) {
        let active = self.viz.is_active();
        let color = if active { Color::Rgb(166, 227, 161) } else { Color::Rgb(108, 112, 134) };
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
            .marker(Marker::Braille)
            .x_bounds([0.0, 1.0])
            .y_bounds([-1.0, 1.0])
            .paint(move |ctx| {
                for w in points.windows(2) {
                    ctx.draw(&CanvasLine { x1: w[0].0, y1: w[0].1, x2: w[1].0, y2: w[1].1, color });
                }
            });
        frame.render_widget(canvas, area);
    }

    fn render_spec(&self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let w = area.width as usize;
        let h = area.height as usize;
        const EIGHTHS: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];
        let mut lines: Vec<Line> = Vec::with_capacity(h);
        for row in 0..h {
            let from_bottom = h - 1 - row;
            let mut spans: Vec<Span> = Vec::with_capacity(w);
            for col in 0..w {
                let band = (col * SPEC_BANDS) / w.max(1);
                let level = self.levels[band.min(SPEC_BANDS - 1)].clamp(0.0, 1.0);
                let filled = level * h as f32;
                let full = filled.floor() as usize;
                let color = bar_color(from_bottom, h);
                let ch = if from_bottom < full {
                    '█'
                } else if from_bottom == full {
                    EIGHTHS[(((filled - full as f32) * 8.0) as usize).min(7)]
                } else {
                    ' '
                };
                spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
            }
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines), area);
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
            Event::KeyPress(k) if self.focused => {
                if matches!(k.code, KeyCode::Char('m') | KeyCode::Tab) {
                    self.mode = if self.mode == Mode::Wave { Mode::Spec } else { Mode::Wave };
                    self.dirty = true;
                    return EventResult::Consumed;
                }
                EventResult::NotConsumed
            }
            Event::Tick => {
                if self.last_frame.elapsed().as_millis() >= 33 && self.viz.is_active() {
                    self.last_frame = Instant::now();
                    if self.mode == Mode::Spec {
                        self.analyze();
                    }
                    self.dirty = true;
                }
                EventResult::NotConsumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn handle_mouse(&mut self, x: u16, y: u16, action: &MouseAction) -> EventResult {
        if let MouseAction::Press(..) | MouseAction::DoubleClick(..) = action {
            if y == self.tab_row {
                if x >= self.tab_wave.0 && x < self.tab_wave.1 {
                    self.mode = Mode::Wave;
                    self.dirty = true;
                    return EventResult::Consumed;
                }
                if x >= self.tab_spec.0 && x < self.tab_spec.1 {
                    self.mode = Mode::Spec;
                    self.dirty = true;
                    return EventResult::Consumed;
                }
            }
        }
        EventResult::NotConsumed
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let border_color = if self.focused { crate::theme::border_focused() } else { crate::theme::border() };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(" Visualizer ", Style::default().fg(crate::theme::primary()).add_modifier(Modifier::BOLD)));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Tab row (WAVE | SPEC), clickable.
        let active = Style::default().fg(Color::Rgb(30, 30, 46)).bg(crate::theme::primary()).add_modifier(Modifier::BOLD);
        let idle = Style::default().fg(Color::Rgb(108, 112, 134));
        let (ws, is) = (if self.mode == Mode::Wave { active } else { idle }, if self.mode == Mode::Spec { active } else { idle });
        let tab_line = Line::from(vec![
            Span::styled(" WAVE ", ws),
            Span::raw(" "),
            Span::styled(" SPEC ", is),
        ]);
        self.tab_row = inner.y - area.y; // local
        self.tab_wave = (inner.x - area.x, inner.x - area.x + 6);
        self.tab_spec = (inner.x - area.x + 7, inner.x - area.x + 13);
        frame.render_widget(Paragraph::new(tab_line), Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 });

        // Content below the tab row.
        if inner.height <= 1 {
            return;
        }
        let content = Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: inner.height - 1 };
        match self.mode {
            Mode::Wave => self.render_wave(frame, content),
            Mode::Spec => self.render_spec(frame, content),
        }
    }

    fn on_focus(&mut self) { self.focused = true; self.dirty = true; }
    fn on_blur(&mut self) { self.focused = false; self.dirty = true; }
}

fn bar_color(from_bottom: usize, height: usize) -> Color {
    let t = if height <= 1 { 0.0 } else { from_bottom as f32 / (height - 1) as f32 };
    if t < 0.6 {
        Color::Rgb(166, 227, 161)
    } else if t < 0.85 {
        Color::Rgb(249, 226, 175)
    } else {
        Color::Rgb(243, 139, 168)
    }
}

fn goertzel(samples: &[f32], freq: f64, fs: f64) -> f32 {
    let n = samples.len();
    if n == 0 || fs <= 0.0 {
        return 0.0;
    }
    let k = freq / fs;
    let coeff = 2.0 * (2.0 * std::f64::consts::PI * k).cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in samples {
        let s0 = x as f64 + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
    (power.max(0.0).sqrt() / (n as f64 / 2.0)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_toggle() {
        let mut s = Oscilloscope::new(AudioViz::new());
        assert!(s.mode == Mode::Wave);
        s.focused = true;
        let _ = s.handle_event(&Event::KeyPress(crate::events::KeyEvent {
            code: KeyCode::Char('m'),
            modifiers: crate::events::KeyModifiers { ctrl: false, alt: false, shift: false },
            mode: crate::events::UiMode::Normal,
        }));
        assert!(s.mode == Mode::Spec);
    }
}
