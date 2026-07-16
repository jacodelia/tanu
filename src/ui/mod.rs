//! UI system — screen composition, layout management, rendering.
//!
//! The `Screen` owns all widgets, manages focus, handles
//! layout splits, and orchestrates differential rendering.
//!
//! Widgets are assigned to named "slots" that determine their
//! position in the layout. Slots are rendered in a fixed order.

use ratatui::layout::Rect;
use ratatui::Frame;
use std::collections::HashMap;

use crate::core::id::WidgetId;
use crate::events::{Event, MouseAction};
use crate::theme::ThemeRegistry;
use crate::widgets::Widget;
use crate::widgets::context_menu::{ContextMenu, MenuItem};
use crate::widgets::dir_picker::DirPicker;
use crate::widgets::popup::Popup;

use self::layout::LayoutManager;

/// Named layout slots for widget placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Slot {
    Tabs,
    SearchBar,
    MainLeft,
    MainRight,
    ProgressBar,
    StatusBar,
    CommandBar,
    /// Equalizer / spectrum panel (right column, between art and scope).
    Eq,
    /// Seek strip (track name + seekable progress) under the visualizer.
    Seek,
}

/// The screen divides the terminal into regions and manages widgets.
pub struct Screen {
    widgets: HashMap<WidgetId, Box<dyn crate::widgets::Widget>>,
    slot_order: Vec<WidgetId>,
    slot_map: HashMap<Slot, WidgetId>,
    focused_widget: Option<WidgetId>,
    theme: ThemeRegistry,
    layout: LayoutManager,
    context_menu: ContextMenu,
    popup: Popup,
    dir_picker: DirPicker,
}

impl Screen {
    pub fn new(theme: ThemeRegistry) -> Self {
        Self {
            widgets: HashMap::new(),
            slot_order: Vec::new(),
            slot_map: HashMap::new(),
            focused_widget: None,
            theme,
            layout: LayoutManager::new(),
            context_menu: ContextMenu::new(),
            popup: Popup::new(),
            dir_picker: DirPicker::new(),
        }
    }

    /// Get a reference to the layout manager.
    pub fn layout(&self) -> &LayoutManager {
        &self.layout
    }

    /// Get a mutable reference to the layout manager.
    pub fn layout_mut(&mut self) -> &mut LayoutManager {
        &mut self.layout
    }

    /// Switch the current layout by name. Marks all widgets dirty.
    pub fn switch_layout(&mut self, name: &str) -> anyhow::Result<()> {
        self.layout.switch(name)?;
        self.mark_dirty();
        Ok(())
    }

    /// Get the current layout name.
    pub fn layout_name(&self) -> &str {
        self.layout.current_name()
    }

    /// Get a reference to the theme registry.
    pub fn theme(&self) -> &ThemeRegistry {
        &self.theme
    }

    /// Get a mutable reference to the theme registry.
    pub fn theme_mut(&mut self) -> &mut ThemeRegistry {
        &mut self.theme
    }

    /// Register a widget with the screen, assigned to a layout slot.
    pub fn add_widget(&mut self, widget: Box<dyn crate::widgets::Widget>, slot: Slot) {
        let id = widget.id();
        self.widgets.insert(id, widget);
        self.slot_map.insert(slot, id);
        self.slot_order.push(id);
    }

    /// Replace the widget in a slot, returning the old widget.
    pub fn replace_widget(&mut self, widget: Box<dyn crate::widgets::Widget>, slot: Slot) -> Option<Box<dyn crate::widgets::Widget>> {
        let id = widget.id();
        let old_id = self.slot_map.insert(slot, id);
        self.slot_order.retain(|&x| x != id);
        self.slot_order.push(id);

        if self.focused_widget == old_id {
            self.focused_widget = Some(id);
        }

        let old = old_id.and_then(|oid| {
            self.slot_order.retain(|&x| x != oid);
            self.widgets.remove(&oid)
        });

        self.widgets.insert(id, widget);
        old
    }

    /// Remove a widget by ID.
    pub fn remove_widget(&mut self, id: WidgetId) {
        if self.focused_widget == Some(id) {
            self.focused_widget = None;
        }
        self.widgets.remove(&id);
        self.slot_order.retain(|&x| x != id);
        self.slot_map.retain(|_, &mut v| v != id);
    }

    /// Returns the widget for a slot, if any.
    pub fn widget_at(&self, slot: Slot) -> Option<&dyn crate::widgets::Widget> {
        self.slot_map
            .get(&slot)
            .and_then(|id| self.widgets.get(id))
            .map(|w| w.as_ref())
    }

    pub fn widget_at_mut(&mut self, slot: Slot) -> Option<&mut Box<dyn crate::widgets::Widget>> {
        self.slot_map.get(&slot).and_then(|id| self.widgets.get_mut(id))
    }

    /// Set the focused widget.
    pub fn set_focus(&mut self, id: Option<WidgetId>) {
        if let Some(current) = self.focused_widget {
            if let Some(widget) = self.widgets.get_mut(&current) {
                widget.on_blur();
            }
        }
        self.focused_widget = id;
        if let Some(id) = id {
            if let Some(widget) = self.widgets.get_mut(&id) {
                widget.on_focus();
            }
        }
    }

    /// Focus the next focusable widget in insertion order.
    pub fn focus_next(&mut self) {
        if self.slot_order.is_empty() {
            return;
        }

        let start = self
            .focused_widget
            .and_then(|id| self.slot_order.iter().position(|&x| x == id))
            .unwrap_or(0);

        for i in 1..=self.slot_order.len() {
            let idx = (start + i) % self.slot_order.len();
            let candidate = self.slot_order[idx];
            if let Some(w) = self.widgets.get(&candidate) {
                if w.is_focusable() {
                    self.set_focus(Some(candidate));
                    return;
                }
            }
        }
    }

    /// Focus the previous focusable widget.
    pub fn focus_previous(&mut self) {
        if self.slot_order.is_empty() {
            return;
        }

        let start = self
            .focused_widget
            .and_then(|id| self.slot_order.iter().position(|&x| x == id))
            .unwrap_or(0);

        for i in (0..self.slot_order.len()).rev() {
            let candidate = self.slot_order[(start + i) % self.slot_order.len()];
            if let Some(w) = self.widgets.get(&candidate) {
                if w.is_focusable() {
                    self.set_focus(Some(candidate));
                    return;
                }
            }
        }
    }

    /// Show a context menu at the given screen position.
    pub fn show_context_menu(&mut self, x: u16, y: u16, items: Vec<MenuItem>) {
        self.context_menu.show(x, y, items);
    }

    /// Show a centered modal menu (title + items).
    pub fn show_modal_menu(&mut self, title: &str, items: Vec<MenuItem>) {
        self.context_menu.show_modal(title, items);
    }

    /// Open the directory picker rooted at `start`.
    pub fn show_dir_picker(&mut self, start: std::path::PathBuf) {
        self.dir_picker.show(start);
    }

    pub fn hide_context_menu(&mut self) {
        self.context_menu.hide();
    }

    pub fn context_menu_visible(&self) -> bool {
        self.context_menu.is_visible()
    }

    pub fn show_popup_info(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup.show_info(title, message);
    }

    /// Show the large About popup with scaled ASCII art.
    pub fn show_popup_about(&mut self, title: impl Into<String>, message: impl Into<String>, art: &'static str) {
        self.popup.show_about(title, message, art);
    }

    /// Show an info popup with Enter/Esc actions (returns commands via Event).
    pub fn show_popup_info_with_actions(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        on_confirm: Option<String>,
        on_cancel: Option<String>,
    ) {
        self.popup.show_info(title, message);
        self.popup.set_on_confirm(on_confirm);
        self.popup.set_on_cancel(on_cancel);
    }

    pub fn show_popup_error(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup.show_error(title, message);
    }

    pub fn show_popup_confirm(&mut self, title: impl Into<String>, message: impl Into<String>, on_confirm: String) {
        self.popup.show_confirm(title, message, on_confirm);
    }

    pub fn show_popup_input(&mut self, title: impl Into<String>, on_confirm: String) {
        self.popup.show_input(title, on_confirm);
    }

    pub fn hide_popup(&mut self) {
        self.popup.hide();
    }

    pub fn popup_visible(&self) -> bool {
        self.popup.is_visible()
    }

    /// Find the topmost widget at the given screen coordinates.
    /// Returns (widget_id, local_x, local_y) if a widget was found.
    pub fn widget_at_screen_pos(&self, x: u16, y: u16) -> Option<(WidgetId, u16, u16)> {
        for &id in self.slot_order.iter().rev() {
            if let Some(widget) = self.widgets.get(&id) {
                let rect = widget.rect();
                if x >= rect.x && x < rect.x.saturating_add(rect.width)
                    && y >= rect.y && y < rect.y.saturating_add(rect.height)
                {
                    let local_x = x.saturating_sub(rect.x);
                    let local_y = y.saturating_sub(rect.y);
                    return Some((id, local_x, local_y));
                }
            }
        }
        None
    }

    /// Handle a global event, dispatching to all widgets (key events)
    /// or routing to the hit-tested widget (mouse events).
    /// Returns any events produced by widgets.
    pub fn handle_event(&mut self, event: &Event) -> Vec<Event> {
        let mut produced = Vec::new();

        if self.dir_picker.is_visible() {
            let result = self.dir_picker.handle_event(event);
            apply_event_result(result, &mut produced);
            // Modal: capture all input while open.
            return produced;
        }

        if self.context_menu.is_visible() {
            let result = self.context_menu.handle_event(event);
            apply_event_result(result, &mut produced);
            if !produced.is_empty() {
                return produced;
            }
        }

        if self.popup.is_visible() {
            let result = self.popup.handle_event(event);
            apply_event_result(result, &mut produced);
            if !produced.is_empty() {
                return produced;
            }
        }

        match event {
            Event::MouseAction(action) => {
                produced.extend(self.route_mouse(action));
            }
            _ => {
                produced.extend(self.broadcast_key_event(event));
            }
        }

        produced
    }

    /// Broadcast a key event to all widgets.
    fn broadcast_key_event(&mut self, event: &Event) -> Vec<Event> {
        let widget_ids: Vec<WidgetId> = self.slot_order.clone();
        let mut produced = Vec::new();

        for id in widget_ids {
            if let Some(widget) = self.widgets.get_mut(&id) {
                let result = widget.handle_event(event);
                apply_event_result(result, &mut produced);
            }
        }

        produced
    }

    /// Route a mouse action to the hit-tested widget or divider.
    fn route_mouse(&mut self, action: &MouseAction) -> Vec<Event> {
        let (x, y) = action.coords();
        let mut produced = Vec::new();

        // Handle divider drag: Press on a divider starts drag
        match action {
            MouseAction::Press(_, x, y) => {
                let area = Rect { x: 0, y: 0, width: u16::MAX, height: u16::MAX };
                if let Some(idx) = self.layout.divider_at(*x, *y, area) {
                    if self.layout.start_drag(idx, *x, *y) {
                        return produced;
                    }
                }
            }
            MouseAction::Drag(_, x, y) | MouseAction::Hold(_, x, y) => {
                if self.layout.is_dragging() {
                    let total = if self.layout.current_name() == "default" { 40 } else { 24 };
                    self.layout.update_drag(*x, *y, total);
                    return produced;
                }
            }
            MouseAction::Release(..) => {
                if self.layout.is_dragging() {
                    self.layout.end_drag();
                    return produced;
                }
            }
            _ => {}
        }

        // Right-click shows context menu
        if matches!(action, MouseAction::RightClick(..)) {
            if let Some((id, _, _)) = self.widget_at_screen_pos(x, y) {
                self.show_context_menu_for(x, y, id);
                return produced;
            }
        }

        // Click outside context menu closes it
        if self.context_menu.is_visible() && action.is_click() {
            self.hide_context_menu();
            return produced;
        }

        if let Some((id, local_x, local_y)) = self.widget_at_screen_pos(x, y) {
            if action.is_click() {
                if let Some(w) = self.widgets.get(&id) {
                    if w.is_focusable() {
                        self.set_focus(Some(id));
                    }
                }
            }

            if let Some(widget) = self.widgets.get_mut(&id) {
                let result = widget.handle_mouse(local_x, local_y, action);
                apply_event_result(result, &mut produced);
            }
        }

        produced
    }

    fn show_context_menu_for(&mut self, x: u16, y: u16, widget_id: WidgetId) {
        let items = self.context_menu_items_for(widget_id);
        self.show_context_menu(x, y, items);
    }

    fn context_menu_items_for(&self, widget_id: WidgetId) -> Vec<MenuItem> {
        let slot = self.slot_map.iter().find(|(_, &id)| id == widget_id).map(|(s, _)| *s);

        let mut items = Vec::new();
        match slot {
            Some(Slot::MainLeft) => {
                items.push(MenuItem { label: "Play".into(), command: "play_selected".into() });
                items.push(MenuItem { label: "Add to Queue".into(), command: "queue_selected".into() });
                items.push(MenuItem { label: "Add to Playlist".into(), command: "add_to_playlist".into() });
            }
            Some(Slot::MainRight) => {
                items.push(MenuItem { label: "Play".into(), command: "play_selected".into() });
                items.push(MenuItem { label: "Remove from Playlist".into(), command: "remove_selected".into() });
            }
            _ => {
                items.push(MenuItem { label: "Refresh".into(), command: "refresh".into() });
            }
        }

        items
    }

    /// Render all dirty widgets using the current layout.
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        if area.width == 0 || area.height == 0 {
            return;
        }

        let regions = self.layout.compute_regions(area);

        // Ratatui hands us a blank buffer each draw, so every widget must
        // render every frame — drawing only the dirty ones blanks the rest.
        // Dirty tracking still gates *whether* we draw at all (see needs_render).
        for (slot, rect) in &regions {
            if let Some(id) = self.slot_map.get(slot) {
                if let Some(widget) = self.widgets.get_mut(id) {
                    widget.set_rect(*rect);
                    widget.render(frame, *rect);
                    widget.mark_clean();
                }
            }
        }

        // Render divider lines
        for (_idx, div_rect) in self.layout.divider_regions(area) {
            use ratatui::widgets::Paragraph;
            let div_style = self.theme.style_for("divider");
            if div_rect.height == 1 {
                let line = "─".repeat(div_rect.width as usize);
                let p = Paragraph::new(line).style(div_style);
                frame.render_widget(p, div_rect);
            } else if div_rect.width == 1 {
                for row in div_rect.y..div_rect.y + div_rect.height {
                    let cell = Rect { x: div_rect.x, y: row, width: 1, height: 1 };
                    if cell.intersects(area) {
                        let p = Paragraph::new("│").style(div_style);
                        frame.render_widget(p, cell);
                    }
                }
            }
        }

        if self.context_menu.is_visible() || self.context_menu.is_dirty() {
            self.context_menu.render(frame, area);
            self.context_menu.mark_clean();
        }
        if self.popup.is_visible() || self.popup.is_dirty() {
            self.popup.render(frame, area);
            self.popup.mark_clean();
        }
        if self.dir_picker.is_visible() || self.dir_picker.is_dirty() {
            self.dir_picker.render(frame, area);
            self.dir_picker.mark_clean();
        }
    }

    /// Mark all widgets as dirty. Call after layout switch, theme change, resize.
    pub fn mark_dirty(&mut self) {
        for widget in self.widgets.values_mut() {
            widget.mark_dirty();
        }
    }

    /// Returns true if any widget needs rendering.
    pub fn needs_render(&self) -> bool {
        if self.context_menu.is_visible() && self.context_menu.is_dirty() {
            return true;
        }
        if self.popup.is_visible() && self.popup.is_dirty() {
            return true;
        }
        if self.dir_picker.is_visible() && self.dir_picker.is_dirty() {
            return true;
        }
        for widget in self.widgets.values() {
            if widget.is_dirty() {
                return true;
            }
        }
        false
    }
}

/// Layout manager provides flexible panel arrangements.
pub mod layout;

fn apply_event_result(result: crate::widgets::EventResult, produced: &mut Vec<Event>) {
    match result {
        crate::widgets::EventResult::Event(e) => {
            produced.push(e);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use crate::widgets::status_bar::StatusBar;

    #[test]
    fn test_screen_creation() {
        let theme = ThemeRegistry::new();
        let screen = Screen::new(theme);
        assert!(screen.focused_widget.is_none());
    }

    #[test]
    fn test_focus_cycle_empty() {
        let theme = ThemeRegistry::new();
        let mut screen = Screen::new(theme);
        screen.focus_next();
        screen.focus_previous();
    }

    #[test]
    fn test_mark_dirty() {
        let theme = ThemeRegistry::new();
        let mut screen = Screen::new(theme);
        screen.mark_dirty();
        for widget in screen.widgets.values() {
            assert!(widget.is_dirty());
        }
    }

    #[test]
    fn test_hit_testing_on_widget() {
        let theme = ThemeRegistry::new();
        let mut screen = Screen::new(theme);
        let status = StatusBar::new();
        screen.add_widget(Box::new(status), Slot::StatusBar);

        // Manually set a rect for hit-testing
        if let Some(widget) = screen.widget_at_mut(Slot::StatusBar) {
            widget.set_rect(Rect { x: 0, y: 20, width: 80, height: 1 });
        }

        let result = screen.widget_at_screen_pos(5, 20);
        assert!(result.is_some());
        let (_, lx, ly) = result.unwrap();
        assert_eq!(lx, 5);
        assert_eq!(ly, 0);
    }

    #[test]
    fn test_hit_testing_outside_widget() {
        let theme = ThemeRegistry::new();
        let mut screen = Screen::new(theme);
        let status = StatusBar::new();
        screen.add_widget(Box::new(status), Slot::StatusBar);

        if let Some(widget) = screen.widget_at_mut(Slot::StatusBar) {
            widget.set_rect(Rect { x: 0, y: 20, width: 80, height: 1 });
        }

        let result = screen.widget_at_screen_pos(5, 5);
        assert!(result.is_none());
    }

    #[test]
    fn test_hit_testing_no_widgets() {
        let theme = ThemeRegistry::new();
        let screen = Screen::new(theme);

        let result = screen.widget_at_screen_pos(5, 5);
        assert!(result.is_none());
    }

    #[test]
    fn test_replace_widget() {
        let theme = ThemeRegistry::new();
        let mut screen = Screen::new(theme);
        let status1 = StatusBar::new();
        screen.add_widget(Box::new(status1), Slot::StatusBar);

        let status2 = StatusBar::new();
        let old = screen.replace_widget(Box::new(status2), Slot::StatusBar);
        assert!(old.is_some());
    }

    #[test]
    fn test_switch_layout() {
        let theme = ThemeRegistry::new();
        let mut screen = Screen::new(theme);
        assert_eq!(screen.layout_name(), "default");
        assert!(screen.switch_layout("compact").is_ok());
        assert_eq!(screen.layout_name(), "compact");
        assert!(screen.switch_layout("nonexistent").is_err());
    }

    #[test]
    fn test_theme_preview_flow() {
        let theme = ThemeRegistry::new();
        let mut screen = Screen::new(theme);
        assert!(!screen.theme().has_preview());
        assert!(screen.theme_mut().preview_theme("dracula").is_ok());
        assert!(screen.theme().has_preview());
        assert_eq!(screen.theme().current_name(), "catppuccin-mocha");
        screen.theme_mut().apply_preview();
        assert!(!screen.theme().has_preview());
        assert_eq!(screen.theme().current_name(), "dracula");
    }

    #[test]
    fn test_layout_manager_integrated() {
        let theme = ThemeRegistry::new();
        let screen = Screen::new(theme);
        let names = screen.layout().list_names();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"focus"));
        assert_eq!(names.len(), 4);
    }
}
