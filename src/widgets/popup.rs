use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, KeyCode};
use crate::widgets::{EventResult, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupType {
    Info,
    Error,
    Confirm,
    Input,
}

pub struct Popup {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    focused: bool,
    visible: bool,
    popup_type: PopupType,
    title: String,
    message: String,
    input_buffer: String,
    on_confirm: Option<String>,
    on_cancel: Option<String>,
    /// ASCII art shown (scaled to fit) in an About popup.
    art: Option<&'static str>,
    /// Screen coords of the `[x]` close button: (row, x_start, x_end). Set on render.
    close_region: Option<(u16, u16, u16)>,
}

impl Default for Popup {
    fn default() -> Self {
        Self::new()
    }
}

impl Popup {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            focused: false,
            visible: false,
            popup_type: PopupType::Info,
            title: String::new(),
            message: String::new(),
            input_buffer: String::new(),
            on_confirm: None,
            on_cancel: None,
            art: None,
            close_region: None,
        }
    }

    /// Close the popup, firing an on-cancel command if one is set.
    fn close(&mut self) -> EventResult {
        if let Some(cmd) = self.on_cancel.take() {
            self.hide();
            return EventResult::Event(Event::Command(cmd));
        }
        self.hide();
        EventResult::Consumed
    }

    pub fn show_info(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup_type = PopupType::Info;
        self.title = title.into();
        self.message = message.into();
        self.art = None;
        self.visible = true;
        self.dirty = true;
    }

    /// Large About popup: ASCII art scaled to fit, plus a caption message.
    pub fn show_about(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        art: &'static str,
    ) {
        self.popup_type = PopupType::Info;
        self.title = title.into();
        self.message = message.into();
        self.art = Some(art);
        self.visible = true;
        self.dirty = true;
    }

    pub fn show_error(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup_type = PopupType::Error;
        self.title = title.into();
        self.message = message.into();
        self.art = None;
        self.visible = true;
        self.dirty = true;
    }

    pub fn show_confirm(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        on_confirm: String,
    ) {
        self.popup_type = PopupType::Confirm;
        self.title = title.into();
        self.message = message.into();
        self.art = None;
        self.on_confirm = Some(on_confirm);
        self.visible = true;
        self.dirty = true;
    }

    pub fn show_input(&mut self, title: impl Into<String>, on_confirm: String) {
        self.popup_type = PopupType::Input;
        self.title = title.into();
        self.message = String::new();
        self.art = None;
        self.input_buffer.clear();
        self.on_confirm = Some(on_confirm);
        self.visible = true;
        self.dirty = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.dirty = true;
    }

    pub fn set_on_confirm(&mut self, cmd: Option<String>) {
        self.on_confirm = cmd;
    }

    pub fn set_on_cancel(&mut self, cmd: Option<String>) {
        self.on_cancel = cmd;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn popup_rect(&self, screen: Rect) -> Rect {
        let (w, h) = if self.art.is_some() {
            // About: tall but narrow — cap the width so it doesn't span the screen.
            (
                60u16.min(screen.width.saturating_sub(4)),
                screen.height.saturating_sub(4),
            )
        } else {
            (
                50u16.min(screen.width.saturating_sub(4)),
                10u16.min(screen.height.saturating_sub(4)),
            )
        };
        let x = screen.width.saturating_sub(w) / 2;
        let y = screen.height.saturating_sub(h) / 2;
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }
}

/// Downscale ASCII art to fit `avail_w` × `avail_h` by nearest-neighbor
/// sampling of rows and columns, centering each sampled row.
fn scale_art(art: &str, avail_w: u16, avail_h: u16) -> Vec<String> {
    let rows: Vec<&str> = art.lines().collect();
    if rows.is_empty() || avail_w == 0 || avail_h == 0 {
        return Vec::new();
    }
    let art_h = rows.len();
    let art_w = rows
        .iter()
        .map(|r| r.chars().count())
        .max()
        .unwrap_or(0)
        .max(1);
    let step = (art_w.div_ceil(avail_w as usize))
        .max(art_h.div_ceil(avail_h as usize))
        .max(1);
    let out_w = art_w.div_ceil(step);
    let mut out = Vec::new();
    let mut r = 0;
    while r < art_h {
        let chars: Vec<char> = rows[r].chars().collect();
        let mut line = String::with_capacity(out_w);
        let mut c = 0;
        while c < art_w {
            line.push(*chars.get(c).unwrap_or(&' '));
            c += step;
        }
        let pad = (avail_w as usize).saturating_sub(line.chars().count()) / 2;
        out.push(format!("{}{}", " ".repeat(pad), line));
        r += step;
    }
    out
}

impl Widget for Popup {
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
        self.dirty || self.visible
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
    fn is_focusable(&self) -> bool {
        self.visible
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        if !self.visible {
            return EventResult::NotConsumed;
        }

        match event {
            Event::KeyPress(key) => match key.code {
                KeyCode::Escape => {
                    if let Some(cmd) = self.on_cancel.take() {
                        self.hide();
                        return EventResult::Event(Event::Command(cmd));
                    }
                    self.hide();
                    EventResult::Consumed
                }
                KeyCode::Enter
                    if self.popup_type == PopupType::Info
                        || self.popup_type == PopupType::Error =>
                {
                    if let Some(cmd) = self.on_confirm.take() {
                        self.hide();
                        return EventResult::Event(Event::Command(cmd));
                    }
                    self.hide();
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    if let Some(cmd) = self.on_confirm.take() {
                        let full_cmd = if self.popup_type == PopupType::Input {
                            format!("{}:{}", cmd, self.input_buffer)
                        } else {
                            cmd
                        };
                        self.hide();
                        return EventResult::Event(Event::Command(full_cmd));
                    }
                    EventResult::Consumed
                }
                KeyCode::Char('y') if self.popup_type == PopupType::Confirm => {
                    if let Some(cmd) = self.on_confirm.take() {
                        self.hide();
                        return EventResult::Event(Event::Command(cmd));
                    }
                    EventResult::Consumed
                }
                KeyCode::Char('n') if self.popup_type == PopupType::Confirm => {
                    self.hide();
                    EventResult::Consumed
                }
                KeyCode::Char(c) if self.popup_type == PopupType::Input => {
                    self.input_buffer.push(c);
                    self.dirty = true;
                    EventResult::Consumed
                }
                KeyCode::Backspace if self.popup_type == PopupType::Input => {
                    self.input_buffer.pop();
                    self.dirty = true;
                    EventResult::Consumed
                }
                _ => EventResult::NotConsumed,
            },
            Event::MouseAction(action) if action.is_click() => {
                let (mx, my) = action.coords();
                if let Some((ry, x0, x1)) = self.close_region {
                    if my == ry && mx >= x0 && mx < x1 {
                        return self.close();
                    }
                }
                EventResult::NotConsumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let popup_rect = self.popup_rect(area);
        let border_color = match self.popup_type {
            PopupType::Error => Color::Rgb(243, 139, 168),
            PopupType::Confirm => Color::Rgb(249, 226, 175),
            PopupType::Input => Color::Rgb(166, 227, 161),
            _ => crate::theme::border_focused(),
        };

        // Clickable [x] close button on the top-right border.
        let close = "[x]";
        let cw = close.chars().count() as u16;
        let x_end = popup_rect.x + popup_rect.width.saturating_sub(1); // corner col
        let x_start = x_end.saturating_sub(cw);
        self.close_region = Some((popup_rect.y, x_start, x_end));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(self.title.clone())
            .title_top(
                Line::from(Span::styled(
                    close,
                    Style::default()
                        .fg(Color::Rgb(243, 139, 168))
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ))
                .right_aligned(),
            )
            .style(Style::default().bg(Color::Rgb(30, 30, 46)));

        let mut lines: Vec<Line> = Vec::new();

        // About: render the ASCII art scaled to the inner area (reserve 3 rows
        // for the caption + close hint).
        if let Some(art) = self.art {
            let inner_w = popup_rect.width.saturating_sub(2);
            let inner_h = popup_rect.height.saturating_sub(2);
            let art_h = inner_h.saturating_sub(4); // caption + copyright + hint + blank
            for row in scale_art(art, inner_w, art_h) {
                lines.push(Line::from(Span::styled(
                    row,
                    Style::default().fg(crate::theme::primary()),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                self.message.clone(),
                Style::default().fg(Color::Rgb(205, 214, 244)),
            )));
            lines.push(Line::from(Span::styled(
                "Copyright (c) 2026 Jorge Codelia",
                Style::default().fg(Color::Rgb(186, 194, 222)),
            )));
            lines.push(Line::from(Span::styled(
                "Press Enter or Esc to close",
                Style::default().fg(Color::Rgb(108, 112, 134)),
            )));
            let paragraph = Paragraph::new(lines)
                .block(block)
                .alignment(Alignment::Left);
            frame.render_widget(Clear, popup_rect);
            frame.render_widget(paragraph, popup_rect);
            return;
        }

        if !self.message.is_empty() {
            lines.push(Line::from(Span::styled(
                self.message.clone(),
                Style::default().fg(Color::Rgb(205, 214, 244)),
            )));
            lines.push(Line::from(""));
        }

        match self.popup_type {
            PopupType::Input => {
                lines.push(Line::from(Span::styled(
                    format!("> {}", self.input_buffer),
                    Style::default().fg(Color::Rgb(166, 227, 161)),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Enter: confirm | Esc: cancel",
                    Style::default().fg(Color::Rgb(108, 112, 134)),
                )));
            }
            PopupType::Confirm => {
                lines.push(Line::from(Span::styled(
                    "[y] Yes  /  [n] No",
                    Style::default().fg(Color::Rgb(249, 226, 175)),
                )));
            }
            _ => {
                lines.push(Line::from(Span::styled(
                    "Press Enter or Esc to close",
                    Style::default().fg(Color::Rgb(108, 112, 134)),
                )));
            }
        }

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Left);

        frame.render_widget(Clear, popup_rect);
        frame.render_widget(paragraph, popup_rect);
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
    fn test_scale_art_fits_bounds() {
        let art = "abcdefghij\n".repeat(20); // 10 wide, 20 tall
        let out = scale_art(&art, 5, 5);
        assert!(!out.is_empty());
        assert!(out.len() <= 5, "rows exceed height: {}", out.len());
        for line in &out {
            assert!(line.chars().count() <= 5, "line exceeds width: {:?}", line);
        }
    }
}
