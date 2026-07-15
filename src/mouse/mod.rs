//! Mouse input handling.
//!
//! Full mouse support: click, double-click, right-click,
//! scroll, drag, selection, and coordinate translation.

use crate::events::MouseAction;
use std::time::{Duration, Instant};

/// Tracks mouse state for detecting gestures.
pub struct MouseHandler {
    pub enabled: bool,
    last_click_position: Option<(u16, u16)>,
    last_click_time: Option<Instant>,
    last_click_button: Option<crate::events::MouseButton>,
    /// Click within this duration counts as double-click.
    double_click_threshold: Duration,
    /// Currently dragging.
    dragging: bool,
    drag_start: Option<(u16, u16)>,
}

impl MouseHandler {
    pub fn new() -> Self {
        Self {
            enabled: true,
            last_click_position: None,
            last_click_time: None,
            last_click_button: None,
            double_click_threshold: Duration::from_millis(400),
            dragging: false,
            drag_start: None,
        }
    }

    /// Process a raw mouse press and return the appropriate action.
    pub fn on_press(
        &mut self,
        button: crate::events::MouseButton,
        x: u16,
        y: u16,
    ) -> MouseAction {
        let now = Instant::now();

        // Detect double-click
        if let (Some(last_pos), Some(last_time), Some(last_btn)) =
            (self.last_click_position, self.last_click_time, self.last_click_button)
        {
            if last_pos == (x, y)
                && last_btn == button
                && now.duration_since(last_time) < self.double_click_threshold
            {
                self.last_click_position = None;
                self.last_click_time = None;
                return MouseAction::DoubleClick(button, x, y);
            }
        }

        self.last_click_position = Some((x, y));
        self.last_click_time = Some(now);
        self.last_click_button = Some(button);
        self.dragging = true;
        self.drag_start = Some((x, y));

        if button == crate::events::MouseButton::Right {
            MouseAction::RightClick(x, y)
        } else {
            MouseAction::Press(button, x, y)
        }
    }

    /// Process a mouse release.
    pub fn on_release(
        &mut self,
        button: crate::events::MouseButton,
        x: u16,
        y: u16,
    ) -> MouseAction {
        self.dragging = false;
        self.drag_start = None;
        MouseAction::Release(button, x, y)
    }

    /// Process a mouse movement (while button held = drag).
    pub fn on_move(&mut self, x: u16, y: u16) -> MouseAction {
        if self.dragging {
            if let Some(start) = self.drag_start {
                if start != (x, y) {
                    MouseAction::Drag(crate::events::MouseButton::Left, x, y)
                } else {
                    MouseAction::Move(x, y)
                }
            } else {
                MouseAction::Move(x, y)
            }
        } else {
            MouseAction::Move(x, y)
        }
    }

    /// Process a scroll event.
    pub fn on_scroll_up(&self, x: u16, y: u16) -> MouseAction {
        MouseAction::ScrollUp(x, y)
    }

    pub fn on_scroll_down(&self, x: u16, y: u16) -> MouseAction {
        MouseAction::ScrollDown(x, y)
    }

    pub fn on_scroll_left(&self, x: u16, y: u16) -> MouseAction {
        MouseAction::ScrollLeft(x, y)
    }

    pub fn on_scroll_right(&self, x: u16, y: u16) -> MouseAction {
        MouseAction::ScrollRight(x, y)
    }

    /// Returns true if a drag operation is in progress.
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }
}

impl Default for MouseHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::MouseButton;

    #[test]
    fn test_single_click() {
        let mut handler = MouseHandler::new();
        let action = handler.on_press(MouseButton::Left, 10, 20);
        assert!(matches!(action, MouseAction::Press(MouseButton::Left, 10, 20)));
    }

    #[test]
    fn test_double_click() {
        let mut handler = MouseHandler::new();
        handler.double_click_threshold = Duration::from_millis(1000);
        handler.on_press(MouseButton::Left, 5, 5);
        let action = handler.on_press(MouseButton::Left, 5, 5);
        assert!(matches!(action, MouseAction::DoubleClick(MouseButton::Left, 5, 5)));
    }

    #[test]
    fn test_right_click() {
        let mut handler = MouseHandler::new();
        let action = handler.on_press(MouseButton::Right, 3, 7);
        assert!(matches!(action, MouseAction::RightClick(3, 7)));
    }

    #[test]
    fn test_scroll() {
        let handler = MouseHandler::new();
        let action = handler.on_scroll_up(0, 10);
        assert!(matches!(action, MouseAction::ScrollUp(0, 10)));
    }

    #[test]
    fn test_drag_detection() {
        let mut handler = MouseHandler::new();
        handler.on_press(MouseButton::Left, 5, 5);
        assert!(handler.is_dragging());
        let action = handler.on_move(6, 6);
        assert!(matches!(action, MouseAction::Drag(MouseButton::Left, 6, 6)));
        handler.on_release(MouseButton::Left, 6, 6);
        assert!(!handler.is_dragging());
    }

    #[test]
    fn test_drag_no_movement() {
        let mut handler = MouseHandler::new();
        handler.on_press(MouseButton::Left, 5, 5);
        let action = handler.on_move(5, 5); // same position
        assert!(matches!(action, MouseAction::Move(5, 5)));
    }
}
