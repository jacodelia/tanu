//! 10-band graphic equalizer (Winamp-style) — sliders that modify the sound.
//!
//! Writes band gains into the shared [`EqState`], which the audio thread reads
//! to filter playback. Keyboard: `←/→` select band, `↑/↓` adjust ±1 dB,
//! `p` cycle preset, `r` reset (flat), `e` on/off. Mouse: click/drag a slider.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::audio::eq::{EqState, EQ_BANDS, EQ_FREQS, EQ_MAX_DB, PRESETS};
use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode, MouseAction};
use crate::widgets::{EventResult, Widget};

pub struct Equalizer {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    eq: EqState,
    selected: usize,
    preset_idx: usize,
    /// Local x range of each band column (rebuilt on render).
    band_x: Vec<(u16, u16)>,
    /// Local y range of the slider track (rebuilt on render).
    track_y: (u16, u16),
}

impl Equalizer {
    pub fn new(eq: EqState) -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            eq,
            selected: 0,
            preset_idx: 0,
            band_x: Vec::new(),
            track_y: (1, 2),
        }
    }

    fn cycle_preset(&mut self) {
        self.preset_idx = (self.preset_idx + 1) % PRESETS.len();
        self.eq.set_all(PRESETS[self.preset_idx].1, 0.0);
        self.dirty = true;
    }

    /// Set the selected band's gain from a local y within the track.
    fn set_gain_from_y(&mut self, y: u16) {
        let (t0, t1) = self.track_y;
        if t1 <= t0 {
            return;
        }
        let yy = y.clamp(t0, t1 - 1);
        let frac = (yy - t0) as f32 / (t1 - 1 - t0).max(1) as f32; // 0 top .. 1 bottom
        let db = EQ_MAX_DB - frac * 2.0 * EQ_MAX_DB;
        self.eq.set_gain(self.selected, db);
        self.dirty = true;
    }
}

impl Widget for Equalizer {
    fn id(&self) -> WidgetId { self.id }
    fn rect(&self) -> Rect { self.rect }
    fn set_rect(&mut self, rect: Rect) { self.rect = rect; }
    fn is_dirty(&self) -> bool { self.dirty }
    fn mark_dirty(&mut self) { self.dirty = true; }
    fn mark_clean(&mut self) { self.dirty = false; }
    fn is_focused(&self) -> bool { self.focused }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        let key = match event {
            Event::KeyPress(k) if self.focused => k,
            _ => return EventResult::NotConsumed,
        };
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => { if self.selected > 0 { self.selected -= 1; self.dirty = true; } EventResult::Consumed }
            KeyCode::Right | KeyCode::Char('l') => { if self.selected + 1 < EQ_BANDS { self.selected += 1; self.dirty = true; } EventResult::Consumed }
            KeyCode::Up | KeyCode::Char('k') => { self.eq.adjust_gain(self.selected, 1.0); self.dirty = true; EventResult::Consumed }
            KeyCode::Down | KeyCode::Char('j') => { self.eq.adjust_gain(self.selected, -1.0); self.dirty = true; EventResult::Consumed }
            KeyCode::Char('p') => { self.cycle_preset(); EventResult::Consumed }
            KeyCode::Char('r') => { self.preset_idx = 0; self.eq.set_all([0.0; EQ_BANDS], 0.0); self.dirty = true; EventResult::Consumed }
            KeyCode::Char('e') => { self.eq.toggle_enabled(); self.dirty = true; EventResult::Consumed }
            _ => EventResult::NotConsumed,
        }
    }

    fn handle_mouse(&mut self, x: u16, y: u16, action: &MouseAction) -> EventResult {
        match action {
            MouseAction::Press(..) | MouseAction::Drag(..) | MouseAction::DoubleClick(..) => {
                if let Some(band) = self.band_x.iter().position(|(s, e)| x >= *s && x < *e) {
                    self.selected = band;
                    self.set_gain_from_y(y);
                    return EventResult::Consumed;
                }
                EventResult::NotConsumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let (gains, _preamp, enabled) = self.eq.snapshot();
        let border_color = if self.focused { Color::Rgb(137, 180, 250) } else { Color::Rgb(69, 71, 90) };
        let title = format!(
            " ≣ EQ · {} · {}",
            PRESETS[self.preset_idx].0,
            if enabled { "on" } else { "OFF" }
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(title, Style::default().fg(Color::Rgb(203, 166, 247)).add_modifier(Modifier::BOLD)))
            .title_bottom(Span::styled(
                " ←→ band · ↑↓ dB · p preset · r flat · e on/off ",
                Style::default().fg(Color::Rgb(108, 112, 134)),
            ));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width < EQ_BANDS as u16 || inner.height < 3 {
            return;
        }

        // Band columns (local coords) + track rows (reserve last row for labels).
        self.band_x.clear();
        let w = inner.width as usize;
        let label_row = inner.height >= 4;
        let track_h = if label_row { inner.height - 1 } else { inner.height } as usize;
        self.track_y = (inner.y - area.y, inner.y - area.y + track_h as u16);

        for b in 0..EQ_BANDS {
            let s = (b * w / EQ_BANDS) as u16 + (inner.x - area.x);
            let e = ((b + 1) * w / EQ_BANDS) as u16 + (inner.x - area.x);
            self.band_x.push((s, e));
        }

        let center = track_h / 2;
        let knob_row = |g: f32| -> usize {
            let frac = ((EQ_MAX_DB - g) / (2.0 * EQ_MAX_DB)).clamp(0.0, 1.0);
            (frac * (track_h.saturating_sub(1)) as f32).round() as usize
        };

        let mut lines: Vec<Line> = Vec::with_capacity(inner.height as usize);
        for row in 0..track_h {
            let mut spans: Vec<Span> = Vec::with_capacity(w);
            for col in 0..w {
                let band = (col * EQ_BANDS) / w;
                let is_selected = band == self.selected;
                // Column is a knob only at the band's center column.
                let band_mid = (band * w / EQ_BANDS + (band + 1) * w / EQ_BANDS) / 2;
                let kr = knob_row(gains[band.min(EQ_BANDS - 1)]);
                let (ch, mut color) = if col == band_mid && row == kr {
                    ('█', if gains[band] >= 0.0 { Color::Rgb(166, 227, 161) } else { Color::Rgb(243, 139, 168) })
                } else if row == center {
                    ('─', Color::Rgb(88, 91, 112))
                } else if col == band_mid {
                    ('│', Color::Rgb(69, 71, 90))
                } else {
                    (' ', Color::Rgb(30, 30, 46))
                };
                if is_selected && ch != ' ' {
                    color = if ch == '█' { color } else { Color::Rgb(137, 180, 250) };
                }
                let mut st = Style::default().fg(color);
                if is_selected {
                    st = st.add_modifier(Modifier::BOLD);
                }
                spans.push(Span::styled(ch.to_string(), st));
            }
            lines.push(Line::from(spans));
        }

        if label_row {
            let mut spans: Vec<Span> = Vec::with_capacity(w);
            let mut placed = vec![' '; w];
            for b in 0..EQ_BANDS {
                let mid = (b * w / EQ_BANDS + (b + 1) * w / EQ_BANDS) / 2;
                let f = EQ_FREQS[b];
                let s = if f >= 1000.0 { format!("{:.0}k", f / 1000.0) } else { format!("{:.0}", f) };
                let start = mid.saturating_sub(s.len() / 2);
                for (i, c) in s.chars().enumerate() {
                    if start + i < w {
                        placed[start + i] = c;
                    }
                }
            }
            for (col, c) in placed.into_iter().enumerate() {
                let band = (col * EQ_BANDS) / w;
                let color = if band == self.selected { Color::Rgb(137, 180, 250) } else { Color::Rgb(108, 112, 134) };
                spans.push(Span::styled(c.to_string(), Style::default().fg(color)));
            }
            lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn on_focus(&mut self) { self.focused = true; self.dirty = true; }
    fn on_blur(&mut self) { self.focused = false; self.dirty = true; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_cycle_sets_eq() {
        let eq = EqState::new();
        let mut w = Equalizer::new(eq.clone());
        w.cycle_preset(); // → Rock
        assert_eq!(w.preset_idx, 1);
        // Non-flat gains applied.
        assert!(eq.snapshot().0.iter().any(|&g| g.abs() > 0.1));
    }
}
