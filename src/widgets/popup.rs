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
        }
    }

    pub fn show_info(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup_type = PopupType::Info;
        self.title = title.into();
        self.message = message.into();
        self.visible = true;
        self.dirty = true;
    }

    pub fn show_error(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup_type = PopupType::Error;
        self.title = title.into();
        self.message = message.into();
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
        self.on_confirm = Some(on_confirm);
        self.visible = true;
        self.dirty = true;
    }

    pub fn show_input(&mut self, title: impl Into<String>, on_confirm: String) {
        self.popup_type = PopupType::Input;
        self.title = title.into();
        self.message = String::new();
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
        let w = 50u16.min(screen.width.saturating_sub(4));
        let h = 10u16.min(screen.height.saturating_sub(4));
        let x = screen.width.saturating_sub(w) / 2;
        let y = screen.height.saturating_sub(h) / 2;
        Rect { x, y, width: w, height: h }
    }
}

impl Widget for Popup {
    fn id(&self) -> WidgetId { self.id }
    fn rect(&self) -> Rect { self.rect }
    fn set_rect(&mut self, rect: Rect) { self.rect = rect; }
    fn is_dirty(&self) -> bool { self.dirty || self.visible }
    fn mark_dirty(&mut self) { self.dirty = true; }
    fn mark_clean(&mut self) { self.dirty = false; }
    fn is_focused(&self) -> bool { self.focused }
    fn is_focusable(&self) -> bool { self.visible }

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
                KeyCode::Enter if self.popup_type == PopupType::Info || self.popup_type == PopupType::Error => {
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

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(self.title.clone())
            .style(Style::default().bg(Color::Rgb(30, 30, 46)));

        let mut lines: Vec<Line> = Vec::new();

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
