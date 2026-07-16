//! Widget system — reusable UI components.
//!
//! Every widget implements the `Widget` trait for rendering
//! and event handling. Widgets support dirty-tracking
//! for differential rendering.

use std::any::Any;

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::Event;

/// Result of event handling by a widget.
#[derive(Debug, Clone, PartialEq)]
pub enum EventResult {
    /// The event was consumed by this widget.
    Consumed,
    /// The event was not consumed; propagate to parent or next widget.
    NotConsumed,
    /// The widget requests focus.
    RequestFocus,
    /// The widget requests a redraw.
    RequestRedraw,
    /// The widget produced an output event.
    Event(Event),
}

/// The core widget trait.
pub trait Widget: Send + Sync + Any {
    /// Returns the widget's unique ID.
    fn id(&self) -> WidgetId;

    /// Returns the widget's current bounding rectangle.
    fn rect(&self) -> Rect;

    /// Sets the widget's bounding rectangle.
    fn set_rect(&mut self, rect: Rect);

    /// Whether the widget needs redrawing.
    fn is_dirty(&self) -> bool;

    /// Marks the widget as needing redraw.
    fn mark_dirty(&mut self);

    /// Marks the widget as clean (after redraw).
    fn mark_clean(&mut self);

    /// Handles a global event. Returns how the event was processed.
    fn handle_event(&mut self, event: &Event) -> EventResult {
        let _ = event;
        EventResult::NotConsumed
    }

    /// Handles a mouse event at widget-local coordinates.
    fn handle_mouse(&mut self, x: u16, y: u16, action: &crate::events::MouseAction) -> EventResult {
        let _ = (x, y, action);
        EventResult::NotConsumed
    }

    /// Renders the widget to the frame.
    fn render(&mut self, frame: &mut Frame, area: Rect);

    /// Called when the widget gains focus.
    fn on_focus(&mut self) {}

    /// Called when the widget loses focus.
    fn on_blur(&mut self) {}

    /// Whether this widget can receive focus.
    fn is_focusable(&self) -> bool {
        true
    }

    /// Whether this widget currently has focus.
    fn is_focused(&self) -> bool;
}

/// Re-export common widget implementations.
pub mod table;
pub mod tree;
pub mod library_view;
pub mod playlist_view;
pub mod queue_view;
pub mod browser_view;
pub mod status_bar;
pub mod progress_bar;
pub mod seek_bar;
pub mod popup;
pub mod context_menu;
pub mod tabs;
pub mod menu_bar;
pub mod oscilloscope;
pub mod album_art;
pub mod equalizer;
pub mod search_bar;
pub mod command_bar;
