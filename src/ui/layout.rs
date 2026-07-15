//! Layout management for UI panels.
//!
//! Supports split views, movable dividers, predefined layouts,
//! and layout persistence to TOML.

use std::collections::HashMap;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use serde::{Deserialize, Serialize};

use crate::ui::Slot;

/// How a single slot is sized.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SlotSize {
    /// Fixed number of rows/columns.
    Fixed(u16),
    /// Percentage of available space (0.0–1.0).
    Percentage(f32),
    /// Minimum space needed (ratatui Length).
    Min(u16),
}

impl SlotSize {
    fn to_constraint(&self) -> Constraint {
        match *self {
            SlotSize::Fixed(n) => Constraint::Length(n),
            SlotSize::Percentage(p) => Constraint::Percentage((p * 100.0) as u16),
            SlotSize::Min(n) => Constraint::Min(n),
        }
    }
}

/// Configuration for a single slot in a layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotConfig {
    pub slot: Slot,
    pub size: SlotSize,
    /// Whether this slot is rendered at all.
    pub visible: bool,
}

/// A named, serializable layout definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutDef {
    pub name: String,
    pub direction: LayoutDirection,
    pub slots: Vec<SlotConfig>,
}

/// Direction for the main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutDirection {
    Vertical,
    Horizontal,
}

impl LayoutDirection {
    fn to_ratatui(&self) -> Direction {
        match self {
            LayoutDirection::Vertical => Direction::Vertical,
            LayoutDirection::Horizontal => Direction::Horizontal,
        }
    }
}

/// State for a drag operation on a divider.
#[derive(Debug, Clone)]
pub struct DragState {
    /// The divider index being dragged (0-based, between slots).
    pub divider_index: usize,
    /// Starting position of the drag (pixels).
    pub start_x: u16,
    pub start_y: u16,
    /// Initial ratio of the first slot.
    pub initial_ratio_first: f32,
    /// Initial ratio of the second slot.
    pub initial_ratio_second: f32,
}

/// The layout manager holds all layout definitions, tracks divider
/// positions for resizable slots, and computes screen regions.
pub struct LayoutManager {
    layouts: HashMap<String, LayoutDef>,
    current: String,
    /// Override ratios for percentage-sized slots: map of (layout_name, slot) -> ratio.
    /// Stored persistently so user adjustments survive layout switches.
    ratio_overrides: HashMap<(String, Slot), f32>,
    /// Active drag operation, if any.
    drag: Option<DragState>,
}

impl LayoutManager {
    pub fn new() -> Self {
        let mut layouts = HashMap::new();
        for def in all_builtin_layouts() {
            layouts.insert(def.name.clone(), def);
        }
        Self {
            current: "default".to_string(),
            layouts,
            ratio_overrides: HashMap::new(),
            drag: None,
        }
    }

    pub fn current_name(&self) -> &str {
        &self.current
    }

    pub fn list_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.layouts.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    pub fn switch(&mut self, name: &str) -> anyhow::Result<()> {
        if self.layouts.contains_key(name) {
            self.current = name.to_string();
            self.drag = None;
            Ok(())
        } else {
            anyhow::bail!("Layout '{}' not found", name)
        }
    }

    /// Register a custom layout (e.g., from a TOML file).
    pub fn register(&mut self, def: LayoutDef) {
        self.layouts.insert(def.name.clone(), def);
    }

    /// Get the current layout definition.
    fn current_def(&self) -> &LayoutDef {
        self.layouts.get(&self.current).expect("current layout not found")
    }

    /// Get the effective size for a slot, accounting for overrides.
    pub fn slot_size(&self, slot: Slot) -> Option<SlotSize> {
        let def = self.current_def();
        for sc in &def.slots {
            if sc.slot == slot && sc.visible {
                if let SlotSize::Percentage(_) = sc.size {
                    if let Some(&ratio) = self.ratio_overrides.get(&(self.current.clone(), slot)) {
                        return Some(SlotSize::Percentage(ratio));
                    }
                }
                return Some(sc.size);
            }
        }
        None
    }

    /// Set the ratio for a percentage-sized slot.
    pub fn set_slot_ratio(&mut self, slot: Slot, ratio: f32) {
        let ratio = ratio.clamp(0.1, 0.9);
        self.ratio_overrides.insert((self.current.clone(), slot), ratio);
    }

    /// Get the overrides map (for serialization).
    pub fn ratio_overrides(&self) -> &HashMap<(String, Slot), f32> {
        &self.ratio_overrides
    }

    /// Set ratio overrides (for deserialization).
    pub fn set_ratio_overrides(&mut self, overrides: HashMap<(String, Slot), f32>) {
        self.ratio_overrides = overrides;
    }

    /// Begin dragging a divider.
    pub fn start_drag(&mut self, divider_index: usize, x: u16, y: u16) -> bool {
        let def = self.current_def();
        let visible: Vec<&SlotConfig> = def.slots.iter().filter(|s| s.visible).collect();

        if divider_index >= visible.len().saturating_sub(1) {
            return false;
        }

        let ratio_first = match visible[divider_index].size {
            SlotSize::Percentage(p) => p,
            _ => return false,
        };
        let ratio_second = match visible[divider_index + 1].size {
            SlotSize::Percentage(p) => p,
            _ => return false,
        };

        self.drag = Some(DragState {
            divider_index,
            start_x: x,
            start_y: y,
            initial_ratio_first: ratio_first,
            initial_ratio_second: ratio_second,
        });
        true
    }

    /// Update a drag operation with new screen coordinates.
    /// Returns true if the layout ratios changed.
    pub fn update_drag(&mut self, x: u16, y: u16, total_size: u16) -> bool {
        let drag = match self.drag.as_ref() {
            Some(d) => d,
            None => return false,
        };

        let is_vertical = self.current_def().direction == LayoutDirection::Vertical;
        let delta = if is_vertical {
            y as i32 - drag.start_y as i32
        } else {
            x as i32 - drag.start_x as i32
        };

        if total_size == 0 {
            return false;
        }

        let delta_ratio = delta as f32 / total_size as f32;
        let total_ratio = drag.initial_ratio_first + drag.initial_ratio_second;
        let new_first = (drag.initial_ratio_first + delta_ratio).clamp(0.05, total_ratio - 0.05);
        let new_second = total_ratio - new_first;

        // Collect slot info before mutating
        let visible: Vec<Slot> = {
            let def = self.current_def();
            def.slots.iter()
                .filter(|s| s.visible)
                .map(|s| s.slot)
                .collect()
        };

        let divider_index = drag.divider_index;
        if divider_index >= visible.len().saturating_sub(1) {
            return false;
        }

        self.set_slot_ratio(visible[divider_index], new_first);
        self.set_slot_ratio(visible[divider_index + 1], new_second);

        true
    }

    /// End the current drag.
    pub fn end_drag(&mut self) {
        self.drag = None;
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Compute the divider regions for the current layout within `area`.
    /// Returns (Slot, Slot, Rect) for each divider line: a 1px strip between two slots.
    pub fn divider_regions(&self, area: Rect) -> Vec<(usize, Rect)> {
        let def = self.current_def();
        let visible: Vec<&SlotConfig> = def.slots.iter().filter(|s| s.visible).collect();
        if visible.len() < 2 {
            return vec![];
        }

        let constraints: Vec<Constraint> = visible.iter().map(|s| {
            match s.size {
                SlotSize::Percentage(_) => {
                    if let Some(&ratio) = self.ratio_overrides.get(&(self.current.clone(), s.slot)) {
                        SlotSize::Percentage(ratio).to_constraint()
                    } else {
                        s.size.to_constraint()
                    }
                }
                _ => s.size.to_constraint(),
            }
        }).collect();

        let regions = Layout::default()
            .direction(def.direction.to_ratatui())
            .constraints(constraints)
            .split(area);

        let mut dividers = Vec::new();
        for i in 0..(regions.len() - 1) {
            let divider = match def.direction {
                LayoutDirection::Vertical => Rect {
                    x: area.x,
                    y: regions[i].y + regions[i].height,
                    width: area.width,
                    height: 1,
                },
                LayoutDirection::Horizontal => Rect {
                    x: regions[i].x + regions[i].width,
                    y: area.y,
                    width: 1,
                    height: area.height,
                },
            };
            // Clamp to area
            if divider.intersects(area) {
                dividers.push((i, divider));
            }
        }
        dividers
    }

    /// Try to find a divider at screen position (x, y). Returns divider index if found.
    pub fn divider_at(&self, x: u16, y: u16, area: Rect) -> Option<usize> {
        for (idx, rect) in self.divider_regions(area) {
            if x >= rect.x && x < rect.x.saturating_add(rect.width)
                && y >= rect.y && y < rect.y.saturating_add(rect.height)
            {
                return Some(idx);
            }
        }
        None
    }

    /// Compute rendering regions for each visible slot within `area`.
    /// Returns a Vec of (Slot, Rect) in display order.
    pub fn compute_regions(&self, area: Rect) -> Vec<(Slot, Rect)> {
        let def = self.current_def();
        let visible: Vec<&SlotConfig> = def.slots.iter().filter(|s| s.visible).collect();
        if visible.is_empty() {
            return vec![];
        }

        let constraints: Vec<Constraint> = visible.iter().map(|s| {
            match s.size {
                SlotSize::Percentage(_) => {
                    if let Some(&ratio) = self.ratio_overrides.get(&(self.current.clone(), s.slot)) {
                        SlotSize::Percentage(ratio).to_constraint()
                    } else {
                        s.size.to_constraint()
                    }
                }
                _ => s.size.to_constraint(),
            }
        }).collect();

        let regions = Layout::default()
            .direction(def.direction.to_ratatui())
            .constraints(constraints)
            .split(area);

        visible.iter().enumerate()
            .filter_map(|(i, sc)| {
                if i < regions.len() {
                    Some((sc.slot, regions[i]))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Load layout definitions from a TOML file.
    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Vec<LayoutDef>> {
        let content = std::fs::read_to_string(path)?;
        let defs: Vec<LayoutDef> = toml::from_str(&content)?;
        Ok(defs)
    }

    /// Save current layout overrides to a TOML file.
    pub fn save_overrides(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let map: HashMap<String, HashMap<String, f32>> = self.ratio_overrides.iter()
            .fold(HashMap::new(), |mut acc, ((layout, slot), ratio)| {
                let slot_key = format!("{:?}", slot).to_lowercase();
                acc.entry(layout.clone())
                    .or_default()
                    .insert(slot_key, *ratio);
                acc
            });

        let content = toml::to_string_pretty(&map)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load ratio overrides from a TOML file.
    pub fn load_overrides(path: &std::path::Path) -> anyhow::Result<HashMap<(String, Slot), f32>> {
        let content = std::fs::read_to_string(path)?;
        let map: HashMap<String, HashMap<String, f32>> = toml::from_str(&content)?;
        let mut result = HashMap::new();
        for (layout_name, slots) in map {
            for (slot_name, ratio) in slots {
                let slot = match slot_name.to_lowercase().as_str() {
                    "tabs" => Slot::Tabs,
                    "searchbar" | "search_bar" => Slot::SearchBar,
                    "mainleft" | "main_left" => Slot::MainLeft,
                    "mainright" | "main_right" => Slot::MainRight,
                    "progressbar" | "progress_bar" => Slot::ProgressBar,
                    "statusbar" | "status_bar" => Slot::StatusBar,
                    "commandbar" | "command_bar" => Slot::CommandBar,
                    _ => continue,
                };
                result.insert((layout_name.clone(), slot), ratio);
            }
        }
        Ok(result)
    }
}

impl Default for LayoutManager {
    fn default() -> Self {
        Self::new()
    }
}

/// All built-in layout definitions.
pub fn all_builtin_layouts() -> Vec<LayoutDef> {
    vec![default_layout(), compact_layout(), wide_layout(), focus_layout()]
}

fn default_layout() -> LayoutDef {
    LayoutDef {
        name: "default".into(),
        direction: LayoutDirection::Vertical,
        slots: vec![
            SlotConfig { slot: Slot::Tabs, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::SearchBar, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::MainLeft, size: SlotSize::Percentage(0.5), visible: true },
            SlotConfig { slot: Slot::MainRight, size: SlotSize::Percentage(0.5), visible: true },
            SlotConfig { slot: Slot::ProgressBar, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::StatusBar, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::CommandBar, size: SlotSize::Fixed(1), visible: true },
        ],
    }
}

fn compact_layout() -> LayoutDef {
    LayoutDef {
        name: "compact".into(),
        direction: LayoutDirection::Vertical,
        slots: vec![
            SlotConfig { slot: Slot::Tabs, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::SearchBar, size: SlotSize::Fixed(0), visible: false },
            SlotConfig { slot: Slot::MainLeft, size: SlotSize::Percentage(1.0), visible: true },
            SlotConfig { slot: Slot::MainRight, size: SlotSize::Percentage(0.0), visible: false },
            SlotConfig { slot: Slot::ProgressBar, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::StatusBar, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::CommandBar, size: SlotSize::Fixed(0), visible: false },
        ],
    }
}

fn wide_layout() -> LayoutDef {
    LayoutDef {
        name: "wide".into(),
        direction: LayoutDirection::Vertical,
        slots: vec![
            SlotConfig { slot: Slot::Tabs, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::SearchBar, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::MainLeft, size: SlotSize::Percentage(0.35), visible: true },
            SlotConfig { slot: Slot::MainRight, size: SlotSize::Percentage(0.65), visible: true },
            SlotConfig { slot: Slot::ProgressBar, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::StatusBar, size: SlotSize::Fixed(1), visible: true },
            SlotConfig { slot: Slot::CommandBar, size: SlotSize::Fixed(1), visible: true },
        ],
    }
}

fn focus_layout() -> LayoutDef {
    LayoutDef {
        name: "focus".into(),
        direction: LayoutDirection::Vertical,
        slots: vec![
            SlotConfig { slot: Slot::Tabs, size: SlotSize::Fixed(0), visible: false },
            SlotConfig { slot: Slot::SearchBar, size: SlotSize::Fixed(0), visible: false },
            SlotConfig { slot: Slot::MainLeft, size: SlotSize::Percentage(1.0), visible: true },
            SlotConfig { slot: Slot::MainRight, size: SlotSize::Percentage(0.0), visible: false },
            SlotConfig { slot: Slot::ProgressBar, size: SlotSize::Fixed(0), visible: false },
            SlotConfig { slot: Slot::StatusBar, size: SlotSize::Fixed(0), visible: false },
            SlotConfig { slot: Slot::CommandBar, size: SlotSize::Fixed(0), visible: false },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_layouts_exist() {
        let layouts = all_builtin_layouts();
        assert_eq!(layouts.len(), 4);
        let names: Vec<&str> = layouts.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"compact"));
        assert!(names.contains(&"wide"));
        assert!(names.contains(&"focus"));
    }

    #[test]
    fn test_layout_manager_creation() {
        let mgr = LayoutManager::new();
        assert_eq!(mgr.current_name(), "default");
        let names = mgr.list_names();
        assert_eq!(names.len(), 4);
    }

    #[test]
    fn test_layout_switch() {
        let mut mgr = LayoutManager::new();
        assert!(mgr.switch("compact").is_ok());
        assert_eq!(mgr.current_name(), "compact");
        assert!(mgr.switch("nonexistent").is_err());
    }

    #[test]
    fn test_slot_ratio_override() {
        let mut mgr = LayoutManager::new();
        mgr.set_slot_ratio(Slot::MainLeft, 0.3);
        let size = mgr.slot_size(Slot::MainLeft).unwrap();
        assert_eq!(size, SlotSize::Percentage(0.3));
    }

    #[test]
    fn test_slot_ratio_clamped() {
        let mut mgr = LayoutManager::new();
        mgr.set_slot_ratio(Slot::MainLeft, 0.0);
        let size = mgr.slot_size(Slot::MainLeft).unwrap();
        assert_eq!(size, SlotSize::Percentage(0.1));
    }

    #[test]
    fn test_compute_regions() {
        let mgr = LayoutManager::new();
        let area = Rect { x: 0, y: 0, width: 80, height: 24 };
        let regions = mgr.compute_regions(area);
        // default has 7 visible slots
        assert_eq!(regions.len(), 7);
    }

    #[test]
    fn test_compact_hides_slots() {
        let mut mgr = LayoutManager::new();
        mgr.switch("compact").unwrap();
        let area = Rect { x: 0, y: 0, width: 80, height: 24 };
        let regions = mgr.compute_regions(area);
        // compact: tabs, mainleft, progressbar, statusbar = 4 visible
        assert_eq!(regions.len(), 4);
    }

    #[test]
    fn test_drag_state() {
        let mgr = LayoutManager::new();
        let area = Rect { x: 0, y: 0, width: 80, height: 24 };
        // divider between Slot::MainLeft and Slot::MainRight (indices 2 and 3)
        let divs = mgr.divider_regions(area);
        assert!(!divs.is_empty());
        // there should be a divider between percentage-sized slots
        let has_main_divider = divs.iter().any(|(idx, _)| *idx == 2);
        assert!(has_main_divider);
    }

    #[test]
    fn test_roundtrip_overrides() {
        let mut mgr = LayoutManager::new();
        mgr.set_slot_ratio(Slot::MainLeft, 0.3);
        mgr.set_slot_ratio(Slot::MainRight, 0.7);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.toml");
        mgr.save_overrides(&path).unwrap();

        let loaded = LayoutManager::load_overrides(&path).unwrap();
        assert_eq!(loaded.get(&("default".into(), Slot::MainLeft)), Some(&0.3));
        assert_eq!(loaded.get(&("default".into(), Slot::MainRight)), Some(&0.7));
    }
}
