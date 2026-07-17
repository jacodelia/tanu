//! Transport panel — radio-cassette style deck with chunky keys and a
//! playback progress bar. Occupies the ProgressBar slot (bottom-right).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, RepeatMode};
use crate::widgets::{EventResult, Widget};

#[derive(Clone, Copy)]
enum Button {
    Prev,
    PlayPause,
    Stop,
    Next,
    Shuffle,
    Repeat,
}

/// Inner label width of each cassette key.
const KEY_W: u16 = 5; // "╔═══╗" = 5 cells

pub struct ProgressBar {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    is_playing: bool,
    shuffle: bool,
    repeat: RepeatMode,
    volume: f32,
    /// Column ranges of each key (widget-local x), rebuilt every render.
    buttons: Vec<(u16, u16, Button)>,
    /// Rows (widget-local y) the keys occupy.
    key_rows: (u16, u16),
    /// Volume bar hit region: (row y, bar_start_x, bar_end_x).
    vol_region: Option<(u16, u16, u16)>,
}

impl ProgressBar {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            is_playing: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            volume: 0.8,
            buttons: Vec::new(),
            key_rows: (1, 4),
            vol_region: None,
        }
    }

    /// (label centered in 3 cells, button, is-active) for each key.
    fn keys(&self) -> [(&'static str, Button, bool); 6] {
        [
            ("◀◀", Button::Prev, false),
            (
                if self.is_playing { " ‖ " } else { " ▶ " },
                Button::PlayPause,
                self.is_playing,
            ),
            (" ■ ", Button::Stop, false),
            ("▶▶", Button::Next, false),
            (" ⇄ ", Button::Shuffle, self.shuffle),
            (
                " ↻ ",
                Button::Repeat,
                !matches!(self.repeat, RepeatMode::Off),
            ),
        ]
    }
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
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
            self.is_playing = state.is_playing;
            self.shuffle = state.shuffle;
            self.repeat = state.repeat;
            self.volume = state.volume;
            self.dirty = true;
            return EventResult::Consumed;
        }
        EventResult::NotConsumed
    }

    fn handle_mouse(&mut self, x: u16, y: u16, action: &crate::events::MouseAction) -> EventResult {
        use crate::events::MouseAction;
        if !matches!(
            action,
            MouseAction::Press(..) | MouseAction::DoubleClick(..)
        ) {
            return EventResult::NotConsumed;
        }
        // Click on the volume bar → set volume by x position.
        if let Some((vy, start, end)) = self.vol_region {
            if y == vy && x >= start && x < end && end > start {
                let vol = (x - start) as f32 / (end - start) as f32;
                return EventResult::Event(Event::SetVolume(vol.clamp(0.0, 1.0)));
            }
        }
        // Accept clicks anywhere on the three key rows.
        if y < self.key_rows.0 || y >= self.key_rows.1 {
            return EventResult::NotConsumed;
        }
        let hit = self
            .buttons
            .iter()
            .find(|(s, e, _)| x >= *s && x < *e)
            .map(|(_, _, b)| *b);
        if let Some(button) = hit {
            let event = match button {
                Button::Prev => Event::Previous,
                Button::PlayPause => Event::TogglePlayPause,
                Button::Stop => Event::Stop,
                Button::Next => Event::Next,
                Button::Shuffle => Event::SetShuffle(!self.shuffle),
                Button::Repeat => Event::SetRepeat(match self.repeat {
                    RepeatMode::Off => RepeatMode::Track,
                    RepeatMode::Track => RepeatMode::Playlist,
                    RepeatMode::Playlist => RepeatMode::Off,
                }),
            };
            return EventResult::Event(event);
        }
        EventResult::NotConsumed
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let deck_bg = Color::Rgb(24, 24, 37);
        let panel = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::theme::border()))
            .title(Span::styled(
                " ▚ TAPE DECK ▞ ",
                Style::default()
                    .fg(Color::Rgb(249, 226, 175))
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(deck_bg));
        let inner = panel.inner(area);
        frame.render_widget(panel, area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Build the three-row key bank (top / label / bottom).
        self.buttons.clear();
        let key_style = Style::default().fg(Color::Rgb(186, 194, 222));
        let active_style = Style::default()
            .fg(Color::Rgb(166, 227, 161))
            .add_modifier(Modifier::BOLD);

        let mut top: Vec<Span> = Vec::new();
        let mut mid: Vec<Span> = Vec::new();
        let mut bot: Vec<Span> = Vec::new();
        let mut x = inner.x;
        for (label, button, active) in self.keys() {
            let style = if active { active_style } else { key_style };
            top.push(Span::styled("╔═══╗", style));
            mid.push(Span::styled(format!("║{:^3}║", label), style));
            bot.push(Span::styled("╚═══╝", style));
            // Store hit regions in widget-local coords (handle_mouse gets local).
            self.buttons.push((x - area.x, x + KEY_W - area.x, button));
            x += KEY_W;
        }

        self.key_rows = (inner.y - area.y, inner.y - area.y + 3);
        let rows = [
            (inner.y, Line::from(top)),
            (inner.y + 1, Line::from(mid)),
            (inner.y + 2, Line::from(bot)),
        ];
        for (row_y, line) in rows {
            if row_y < inner.y + inner.height {
                let r = Rect {
                    x: inner.x,
                    y: row_y,
                    width: inner.width,
                    height: 1,
                };
                frame.render_widget(Paragraph::new(line).style(Style::default().bg(deck_bg)), r);
            }
        }

        // Volume bar on row 3 (clickable; + / - keys also adjust it).
        // The playback progress lives in the seek strip under the visualizer.
        self.vol_region = None;
        let bg = Style::default().bg(deck_bg);
        if inner.height >= 4 {
            let label = " VOL ";
            let pct = (self.volume * 100.0).round() as u16;
            let suffix = format!(" {:>3}%", pct);
            let bar_w = inner
                .width
                .saturating_sub(label.chars().count() as u16 + suffix.chars().count() as u16 + 2)
                .max(4);
            let filled = (self.volume.clamp(0.0, 1.0) * bar_w as f32) as usize;
            let empty = bar_w as usize - filled;
            let bar_start = inner.x + label.chars().count() as u16 + 1; // after "▐"
            let row_y = inner.y + 3;
            // Local coords for hit-testing.
            self.vol_region = Some((
                row_y - area.y,
                bar_start - area.x,
                bar_start + bar_w - area.x,
            ));
            let line = Line::from(vec![
                Span::styled(label, Style::default().fg(Color::Rgb(249, 226, 175))),
                Span::styled("▐", Style::default().fg(crate::theme::border())),
                Span::styled(
                    "▓".repeat(filled),
                    Style::default().fg(crate::theme::border_focused()),
                ),
                Span::styled(
                    "░".repeat(empty),
                    Style::default().fg(crate::theme::border()),
                ),
                Span::styled("▌", Style::default().fg(crate::theme::border())),
                Span::styled(suffix, Style::default().fg(Color::Rgb(186, 194, 222))),
            ]);
            let r = Rect {
                x: inner.x,
                y: row_y,
                width: inner.width,
                height: 1,
            };
            frame.render_widget(Paragraph::new(line).style(bg), r);
        }
    }
}
