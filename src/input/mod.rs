//! Input handling — keyboard and mouse event translation.
//!
//! Translates raw crossterm events into Tanu's event types,
//! applying keybinding configuration and mode-dependent mapping.

use crate::config::BindingsConfig;
use crate::events::{Event, KeyCode, KeyEvent, KeyModifiers, MouseAction, MouseButton, UiMode};

/// Converts a crossterm `KeyEvent` into Tanu's `KeyEvent`.
pub fn from_crossterm_key(key: &crossterm::event::KeyEvent, mode: UiMode) -> KeyEvent {
    let code = match key.code {
        crossterm::event::KeyCode::Char(' ') => KeyCode::Space,
        crossterm::event::KeyCode::Char(c) => KeyCode::Char(c),
        crossterm::event::KeyCode::Enter => KeyCode::Enter,
        crossterm::event::KeyCode::Esc => KeyCode::Escape,
        crossterm::event::KeyCode::Backspace => KeyCode::Backspace,
        crossterm::event::KeyCode::Tab => KeyCode::Tab,
        crossterm::event::KeyCode::Left => KeyCode::Left,
        crossterm::event::KeyCode::Right => KeyCode::Right,
        crossterm::event::KeyCode::Up => KeyCode::Up,
        crossterm::event::KeyCode::Down => KeyCode::Down,
        crossterm::event::KeyCode::Home => KeyCode::Home,
        crossterm::event::KeyCode::End => KeyCode::End,
        crossterm::event::KeyCode::PageUp => KeyCode::PageUp,
        crossterm::event::KeyCode::PageDown => KeyCode::PageDown,
        crossterm::event::KeyCode::Delete => KeyCode::Delete,
        crossterm::event::KeyCode::Insert => KeyCode::Insert,
        crossterm::event::KeyCode::F(n) => KeyCode::F(n),
        _ => KeyCode::Null,
    };

    KeyEvent {
        code,
        modifiers: KeyModifiers {
            ctrl: key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL),
            alt: key.modifiers.contains(crossterm::event::KeyModifiers::ALT),
            shift: key
                .modifiers
                .contains(crossterm::event::KeyModifiers::SHIFT),
        },
        mode,
    }
}

/// Converts a crossterm `MouseEvent` into Tanu's `Event`.
pub fn from_crossterm_mouse(mouse: &crossterm::event::MouseEvent) -> Option<Event> {
    use crossterm::event::MouseEventKind;

    let column = mouse.column;
    let row = mouse.row;

    let action = match mouse.kind {
        MouseEventKind::Down(button) => {
            let b = convert_mouse_button(button);
            MouseAction::Press(b, column, row)
        }
        MouseEventKind::Up(button) => {
            let b = convert_mouse_button(button);
            MouseAction::Release(b, column, row)
        }
        MouseEventKind::Drag(button) => {
            let b = convert_mouse_button(button);
            MouseAction::Drag(b, column, row)
        }
        MouseEventKind::Moved => MouseAction::Move(column, row),
        MouseEventKind::ScrollDown => MouseAction::ScrollDown(column, row),
        MouseEventKind::ScrollUp => MouseAction::ScrollUp(column, row),
        MouseEventKind::ScrollLeft => MouseAction::ScrollLeft(column, row),
        MouseEventKind::ScrollRight => MouseAction::ScrollRight(column, row),
    };

    Some(Event::MouseAction(action))
}

pub fn convert_mouse_button(button: crossterm::event::MouseButton) -> MouseButton {
    match button {
        crossterm::event::MouseButton::Left => MouseButton::Left,
        crossterm::event::MouseButton::Right => MouseButton::Right,
        crossterm::event::MouseButton::Middle => MouseButton::Middle,
    }
}

/// The input handler bridges crossterm events to Tanu events.
pub struct InputHandler {
    bindings: BindingsConfig,
    current_mode: UiMode,
}

impl InputHandler {
    pub fn new(bindings: BindingsConfig) -> Self {
        Self {
            bindings,
            current_mode: UiMode::Normal,
        }
    }

    pub fn current_mode(&self) -> UiMode {
        self.current_mode
    }

    pub fn set_mode(&mut self, mode: UiMode) {
        self.current_mode = mode;
    }

    /// Translate a key event and current mode into an action.
    pub fn translate_key(&self, key_event: KeyEvent) -> Option<Event> {
        let bindings = match self.current_mode {
            UiMode::Normal => &self.bindings.normal,
            UiMode::Insert => &self.bindings.insert,
            UiMode::Command => &self.bindings.command,
            UiMode::Visual => &self.bindings.visual,
            _ => &self.bindings.normal,
        };

        let key_str = key_to_string(&key_event);
        for binding in bindings {
            if binding.key == key_str {
                return action_to_event(&binding.action);
            }
        }
        None
    }

    /// Translate a mouse event into a Tanu MouseAction.
    pub fn translate_mouse(&self, button: MouseButton, x: u16, y: u16) -> Event {
        Event::MouseAction(MouseAction::Press(button, x, y))
    }

    pub fn reload_bindings(&mut self, bindings: BindingsConfig) {
        self.bindings = bindings;
    }
}

/// Converts a key event to a string representation for binding matching.
pub fn key_to_string(event: &KeyEvent) -> String {
    let mut parts: Vec<String> = Vec::new();

    if event.modifiers.ctrl {
        parts.push("ctrl".to_string());
    }
    if event.modifiers.alt {
        parts.push("alt".to_string());
    }
    if event.modifiers.shift {
        parts.push("shift".to_string());
    }

    match &event.code {
        KeyCode::Char(c) => parts.push(c.to_string().to_lowercase()),
        KeyCode::Enter => parts.push("enter".to_string()),
        KeyCode::Escape => parts.push("escape".to_string()),
        KeyCode::Backspace => parts.push("backspace".to_string()),
        KeyCode::Tab => parts.push("tab".to_string()),
        KeyCode::Space => parts.push("space".to_string()),
        KeyCode::Left => parts.push("left".to_string()),
        KeyCode::Right => parts.push("right".to_string()),
        KeyCode::Up => parts.push("up".to_string()),
        KeyCode::Down => parts.push("down".to_string()),
        KeyCode::Home => parts.push("home".to_string()),
        KeyCode::End => parts.push("end".to_string()),
        KeyCode::PageUp => parts.push("pageup".to_string()),
        KeyCode::PageDown => parts.push("pagedown".to_string()),
        KeyCode::Delete => parts.push("delete".to_string()),
        KeyCode::Insert => parts.push("insert".to_string()),
        KeyCode::F(n) => parts.push(format!("f{}", n)),
        KeyCode::Null => parts.push("null".to_string()),
    }

    parts.join("+")
}

/// Maps an action name to an event.
fn action_to_event(action: &str) -> Option<Event> {
    match action {
        "scroll_down" => Some(Event::KeyPress(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
            },
            mode: UiMode::Normal,
        })),
        "scroll_up" => Some(Event::KeyPress(KeyEvent {
            code: KeyCode::Up,
            modifiers: KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
            },
            mode: UiMode::Normal,
        })),
        "scroll_top" => Some(Event::KeyPress(KeyEvent {
            code: KeyCode::Home,
            modifiers: KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
            },
            mode: UiMode::Normal,
        })),
        "scroll_bottom" => Some(Event::KeyPress(KeyEvent {
            code: KeyCode::End,
            modifiers: KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
            },
            mode: UiMode::Normal,
        })),
        "search" => Some(Event::ModeChanged(UiMode::Search)),
        "command_mode" => Some(Event::ModeChanged(UiMode::Command)),
        "toggle_play_pause" => Some(Event::TogglePlayPause),
        "smart_play_pause" => Some(Event::Command("smart_play_pause".to_string())),
        "volume_up" => Some(Event::Command("volume_up".to_string())),
        "volume_down" => Some(Event::Command("volume_down".to_string())),
        // Forward Enter to the focused widget (browser plays the file, etc.).
        "select" => Some(Event::KeyPress(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
            },
            mode: UiMode::Normal,
        })),
        "next_view" => Some(Event::Command("next_view".to_string())),
        "previous_view" => Some(Event::Command("previous_view".to_string())),
        "quit" => Some(Event::Quit),
        "normal_mode" => Some(Event::ModeChanged(UiMode::Normal)),
        "execute_command" => Some(Event::Command("execute".to_string())),
        "yank" => Some(Event::Command("yank".to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key_event() -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
            },
            mode: UiMode::Normal,
        }
    }

    #[test]
    fn test_key_to_string_char() {
        let evt = test_key_event();
        assert_eq!(key_to_string(&evt), "j");
    }

    #[test]
    fn test_key_to_string_ctrl() {
        let evt = KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers {
                ctrl: true,
                alt: false,
                shift: false,
            },
            mode: UiMode::Normal,
        };
        assert_eq!(key_to_string(&evt), "ctrl+c");
    }

    #[test]
    fn test_key_to_string_special() {
        let evt = KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
            },
            mode: UiMode::Normal,
        };
        assert_eq!(key_to_string(&evt), "enter");
    }

    #[test]
    fn test_translate_j_key() {
        let handler = InputHandler::new(BindingsConfig::default_bindings());
        let evt = test_key_event();
        let result = handler.translate_key(evt);
        assert!(result.is_some());
    }

    #[test]
    fn test_mode_switching() {
        let mut handler = InputHandler::new(BindingsConfig::default_bindings());
        assert_eq!(handler.current_mode(), UiMode::Normal);
        handler.set_mode(UiMode::Command);
        assert_eq!(handler.current_mode(), UiMode::Command);
    }
}
