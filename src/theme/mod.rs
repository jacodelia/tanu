//! Theme system with hot-swap support.
//!
//! Themes are TOML files mapping semantic color names to
//! RGB values. Themes can be swapped at runtime.

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// Global primary/accent color for tanu's typography (panel titles, brand).
/// Packed 0xRRGGBB. Default: Catppuccin mauve. Changed from EDIT → Text Color.
static PRIMARY: AtomicU32 = AtomicU32::new(0xCBA6F7);

/// Primary color palette offered in the EDIT → Text Color menu (label, hex).
pub const PRIMARY_PALETTE: &[(&str, &str)] = &[
    ("Lavender", "#cba6f7"),
    ("Red", "#f38ba8"),
    ("Orange", "#fab387"),
    ("Yellow", "#f9e2af"),
    ("Green", "#a6e3a1"),
    ("Teal", "#94e2d5"),
    ("Blue", "#89b4fa"),
    ("Magenta", "#f5c2e7"),
    ("White", "#cdd6f4"),
];

/// Set the global primary color from a packed 0xRRGGBB value.
pub fn set_primary(rgb: u32) {
    PRIMARY.store(rgb & 0xFF_FF_FF, Ordering::Relaxed);
}

/// Set the primary color from a `#rrggbb` string. Returns false if unparseable.
pub fn set_primary_hex(hex: &str) -> bool {
    match parse_color(hex) {
        Some(Color::Rgb(r, g, b)) => {
            set_primary((r as u32) << 16 | (g as u32) << 8 | b as u32);
            true
        }
        _ => false,
    }
}

/// The current global primary/accent color (titles, brand, focus, selection).
pub fn primary() -> Color {
    let v = PRIMARY.load(Ordering::Relaxed);
    Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Primary scaled toward black by `factor` (0..1). Used to derive dimmer
/// palette tints (borders, dividers) from the chosen primary color.
fn scaled(factor: f32) -> Color {
    let v = PRIMARY.load(Ordering::Relaxed);
    let s = |shift: u32| ((((v >> shift) & 0xFF) as f32) * factor).round() as u8;
    Color::Rgb(s(16), s(8), s(0))
}

/// Focused/active border color = the full primary.
pub fn border_focused() -> Color {
    primary()
}

/// Idle panel border / divider color — a dim tint of the primary so the whole
/// UI (borders, tree, tape deck) shifts hue with the chosen color.
pub fn border() -> Color {
    scaled(0.42)
}

/// A theme defines colors for every semantic UI element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub author: Option<String>,
    pub colors: HashMap<String, ColorDef>,
}

/// A color definition: foreground, background, and modifiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorDef {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
}

impl ColorDef {
    /// Convert this definition to a ratatui `Style`.
    pub fn to_style(&self) -> Style {
        let mut style = Style::default();

        if let Some(ref fg) = self.fg {
            if let Some(color) = parse_color(fg) {
                style = style.fg(color);
            }
        }
        if let Some(ref bg) = self.bg {
            if let Some(color) = parse_color(bg) {
                style = style.bg(color);
            }
        }
        if self.bold.unwrap_or(false) {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.italic.unwrap_or(false) {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.underline.unwrap_or(false) {
            style = style.add_modifier(Modifier::UNDERLINED);
        }

        style
    }
}

/// Parse a hex or named color into a ratatui `Color`.
pub fn parse_color(s: &str) -> Option<Color> {
    if s.starts_with('#') {
        let hex = &s[1..];
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }

    // Named colors
    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        "reset" | "default" => Some(Color::Reset),
        _ => None,
    }
}

/// A theme registry holds all loaded themes.
pub struct ThemeRegistry {
    themes: HashMap<String, Theme>,
    current: String,
    preview: Option<String>,
}

impl ThemeRegistry {
    pub fn new() -> Self {
        let all = Theme::all_builtin();
        let current = all[0].name.clone();
        Self {
            current,
            preview: None,
            themes: {
                let mut m = HashMap::new();
                for theme in all {
                    m.insert(theme.name.clone(), theme);
                }
                m
            },
        }
    }

    pub fn current(&self) -> &Theme {
        self.themes.get(&self.current).unwrap()
    }

    pub fn current_name(&self) -> &str {
        &self.current
    }

    pub fn list_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.themes.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    pub fn load(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = std::fs::read_to_string(path)?;
        let theme: Theme = toml::from_str(&content)?;
        self.themes.insert(theme.name.clone(), theme);
        Ok(())
    }

    pub fn switch(&mut self, name: &str) -> anyhow::Result<()> {
        if self.themes.contains_key(name) {
            self.current = name.to_string();
            self.preview = None;
            Ok(())
        } else {
            anyhow::bail!("Theme '{}' not found", name)
        }
    }

    /// Preview a theme without applying it.
    pub fn preview_theme(&mut self, name: &str) -> anyhow::Result<()> {
        if self.themes.contains_key(name) {
            self.preview = Some(name.to_string());
            Ok(())
        } else {
            anyhow::bail!("Theme '{}' not found", name)
        }
    }

    /// Cancel the current preview (revert to the active theme).
    pub fn cancel_preview(&mut self) {
        self.preview = None;
    }

    /// Apply the previewed theme (commit).
    pub fn apply_preview(&mut self) {
        if let Some(ref name) = self.preview.take() {
            self.current = name.clone();
        }
    }

    /// Whether a preview is active.
    pub fn has_preview(&self) -> bool {
        self.preview.is_some()
    }

    /// Active effective theme (either preview or current).
    fn effective(&self) -> &Theme {
        let key = self.preview.as_ref().unwrap_or(&self.current);
        self.themes.get(key).unwrap()
    }

    pub fn style_for(&self, element: &str) -> Style {
        self.effective()
            .colors
            .get(element)
            .map(|def| def.to_style())
            .unwrap_or_default()
    }
}

impl Theme {
    /// Returns all built-in themes.
    pub fn all_builtin() -> Vec<Theme> {
        vec![
            Theme::catppuccin_mocha(),
            Theme::catppuccin_latte(),
            Theme::gruvbox_dark(),
            Theme::gruvbox_light(),
            Theme::nord(),
            Theme::tokyonight(),
            Theme::dracula(),
            Theme::solarized(),
        ]
    }

    /// Find a built-in theme by name.
    pub fn builtin(name: &str) -> Option<Theme> {
        match name.to_lowercase().as_str() {
            "catppuccin-mocha" | "catppuccin_mocha" => Some(Theme::catppuccin_mocha()),
            "catppuccin-latte" | "catppuccin_latte" => Some(Theme::catppuccin_latte()),
            "gruvbox-dark" | "gruvbox_dark" => Some(Theme::gruvbox_dark()),
            "gruvbox-light" | "gruvbox_light" => Some(Theme::gruvbox_light()),
            "nord" => Some(Theme::nord()),
            "tokyonight" | "tokyo-night" | "tokyo_night" => Some(Theme::tokyonight()),
            "dracula" => Some(Theme::dracula()),
            "solarized" => Some(Theme::solarized()),
            _ => None,
        }
    }

    fn catppuccin_mocha() -> Theme {
        let mut colors = HashMap::new();
        colors.insert("background".to_string(), ColorDef {
            fg: Some("#cdd6f4".to_string()), bg: Some("#1e1e2e".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("statusbar".to_string(), ColorDef {
            fg: Some("#1e1e2e".to_string()), bg: Some("#89b4fa".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("progressbar".to_string(), ColorDef {
            fg: Some("#1e1e2e".to_string()), bg: Some("#a6e3a1".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.selected".to_string(), ColorDef {
            fg: Some("#1e1e2e".to_string()), bg: Some("#89b4fa".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.highlight".to_string(), ColorDef {
            fg: Some("#89b4fa".to_string()), bg: None,
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("divider".to_string(), ColorDef {
            fg: Some("#45475a".to_string()), bg: None,
            bold: None, italic: None, underline: None,
        });
        colors.insert("command_bar".to_string(), ColorDef {
            fg: Some("#cdd6f4".to_string()), bg: Some("#313244".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("tab.active".to_string(), ColorDef {
            fg: Some("#1e1e2e".to_string()), bg: Some("#cba6f7".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("tab.inactive".to_string(), ColorDef {
            fg: Some("#6c7086".to_string()), bg: Some("#313244".to_string()),
            bold: None, italic: None, underline: None,
        });
        Theme { name: "catppuccin-mocha".into(), author: Some("Tanu".into()), colors }
    }

    fn catppuccin_latte() -> Theme {
        let mut colors = HashMap::new();
        colors.insert("background".to_string(), ColorDef {
            fg: Some("#4c4f69".to_string()), bg: Some("#eff1f5".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("statusbar".to_string(), ColorDef {
            fg: Some("#eff1f5".to_string()), bg: Some("#1e66f5".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("progressbar".to_string(), ColorDef {
            fg: Some("#eff1f5".to_string()), bg: Some("#40a02b".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.selected".to_string(), ColorDef {
            fg: Some("#eff1f5".to_string()), bg: Some("#1e66f5".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.highlight".to_string(), ColorDef {
            fg: Some("#1e66f5".to_string()), bg: None,
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("divider".to_string(), ColorDef {
            fg: Some("#ccd0da".to_string()), bg: None,
            bold: None, italic: None, underline: None,
        });
        colors.insert("command_bar".to_string(), ColorDef {
            fg: Some("#4c4f69".to_string()), bg: Some("#ccd0da".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("tab.active".to_string(), ColorDef {
            fg: Some("#eff1f5".to_string()), bg: Some("#8839ef".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("tab.inactive".to_string(), ColorDef {
            fg: Some("#9ca0b0".to_string()), bg: Some("#ccd0da".to_string()),
            bold: None, italic: None, underline: None,
        });
        Theme { name: "catppuccin-latte".into(), author: Some("Tanu".into()), colors }
    }

    fn gruvbox_dark() -> Theme {
        let mut colors = HashMap::new();
        colors.insert("background".to_string(), ColorDef {
            fg: Some("#ebdbb2".to_string()), bg: Some("#282828".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("statusbar".to_string(), ColorDef {
            fg: Some("#282828".to_string()), bg: Some("#458588".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("progressbar".to_string(), ColorDef {
            fg: Some("#282828".to_string()), bg: Some("#98971a".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.selected".to_string(), ColorDef {
            fg: Some("#282828".to_string()), bg: Some("#458588".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.highlight".to_string(), ColorDef {
            fg: Some("#83a598".to_string()), bg: None,
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("divider".to_string(), ColorDef {
            fg: Some("#3c3836".to_string()), bg: None,
            bold: None, italic: None, underline: None,
        });
        colors.insert("command_bar".to_string(), ColorDef {
            fg: Some("#ebdbb2".to_string()), bg: Some("#3c3836".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("tab.active".to_string(), ColorDef {
            fg: Some("#282828".to_string()), bg: Some("#d3869b".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("tab.inactive".to_string(), ColorDef {
            fg: Some("#a89984".to_string()), bg: Some("#3c3836".to_string()),
            bold: None, italic: None, underline: None,
        });
        Theme { name: "gruvbox-dark".into(), author: Some("Tanu".into()), colors }
    }

    fn gruvbox_light() -> Theme {
        let mut colors = HashMap::new();
        colors.insert("background".to_string(), ColorDef {
            fg: Some("#3c3836".to_string()), bg: Some("#fbf1c7".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("statusbar".to_string(), ColorDef {
            fg: Some("#fbf1c7".to_string()), bg: Some("#076678".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("progressbar".to_string(), ColorDef {
            fg: Some("#fbf1c7".to_string()), bg: Some("#79740e".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.selected".to_string(), ColorDef {
            fg: Some("#fbf1c7".to_string()), bg: Some("#076678".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.highlight".to_string(), ColorDef {
            fg: Some("#076678".to_string()), bg: None,
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("divider".to_string(), ColorDef {
            fg: Some("#ebdbb2".to_string()), bg: None,
            bold: None, italic: None, underline: None,
        });
        colors.insert("command_bar".to_string(), ColorDef {
            fg: Some("#3c3836".to_string()), bg: Some("#ebdbb2".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("tab.active".to_string(), ColorDef {
            fg: Some("#fbf1c7".to_string()), bg: Some("#8f3f71".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("tab.inactive".to_string(), ColorDef {
            fg: Some("#7c6f64".to_string()), bg: Some("#ebdbb2".to_string()),
            bold: None, italic: None, underline: None,
        });
        Theme { name: "gruvbox-light".into(), author: Some("Tanu".into()), colors }
    }

    fn nord() -> Theme {
        let mut colors = HashMap::new();
        colors.insert("background".to_string(), ColorDef {
            fg: Some("#d8dee9".to_string()), bg: Some("#2e3440".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("statusbar".to_string(), ColorDef {
            fg: Some("#2e3440".to_string()), bg: Some("#81a1c1".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("progressbar".to_string(), ColorDef {
            fg: Some("#2e3440".to_string()), bg: Some("#a3be8c".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.selected".to_string(), ColorDef {
            fg: Some("#2e3440".to_string()), bg: Some("#81a1c1".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.highlight".to_string(), ColorDef {
            fg: Some("#88c0d0".to_string()), bg: None,
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("divider".to_string(), ColorDef {
            fg: Some("#4c566a".to_string()), bg: None,
            bold: None, italic: None, underline: None,
        });
        colors.insert("command_bar".to_string(), ColorDef {
            fg: Some("#d8dee9".to_string()), bg: Some("#4c566a".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("tab.active".to_string(), ColorDef {
            fg: Some("#2e3440".to_string()), bg: Some("#b48ead".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("tab.inactive".to_string(), ColorDef {
            fg: Some("#616e88".to_string()), bg: Some("#4c566a".to_string()),
            bold: None, italic: None, underline: None,
        });
        Theme { name: "nord".into(), author: Some("Tanu".into()), colors }
    }

    fn tokyonight() -> Theme {
        let mut colors = HashMap::new();
        colors.insert("background".to_string(), ColorDef {
            fg: Some("#c0caf5".to_string()), bg: Some("#1a1b26".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("statusbar".to_string(), ColorDef {
            fg: Some("#1a1b26".to_string()), bg: Some("#7aa2f7".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("progressbar".to_string(), ColorDef {
            fg: Some("#1a1b26".to_string()), bg: Some("#9ece6a".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.selected".to_string(), ColorDef {
            fg: Some("#1a1b26".to_string()), bg: Some("#7aa2f7".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.highlight".to_string(), ColorDef {
            fg: Some("#7dcfff".to_string()), bg: None,
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("divider".to_string(), ColorDef {
            fg: Some("#292e42".to_string()), bg: None,
            bold: None, italic: None, underline: None,
        });
        colors.insert("command_bar".to_string(), ColorDef {
            fg: Some("#c0caf5".to_string()), bg: Some("#292e42".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("tab.active".to_string(), ColorDef {
            fg: Some("#1a1b26".to_string()), bg: Some("#bb9af7".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("tab.inactive".to_string(), ColorDef {
            fg: Some("#565f89".to_string()), bg: Some("#292e42".to_string()),
            bold: None, italic: None, underline: None,
        });
        Theme { name: "tokyonight".into(), author: Some("Tanu".into()), colors }
    }

    fn dracula() -> Theme {
        let mut colors = HashMap::new();
        colors.insert("background".to_string(), ColorDef {
            fg: Some("#f8f8f2".to_string()), bg: Some("#282a36".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("statusbar".to_string(), ColorDef {
            fg: Some("#282a36".to_string()), bg: Some("#8be9fd".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("progressbar".to_string(), ColorDef {
            fg: Some("#282a36".to_string()), bg: Some("#50fa7b".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.selected".to_string(), ColorDef {
            fg: Some("#282a36".to_string()), bg: Some("#8be9fd".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.highlight".to_string(), ColorDef {
            fg: Some("#ff79c6".to_string()), bg: None,
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("divider".to_string(), ColorDef {
            fg: Some("#44475a".to_string()), bg: None,
            bold: None, italic: None, underline: None,
        });
        colors.insert("command_bar".to_string(), ColorDef {
            fg: Some("#f8f8f2".to_string()), bg: Some("#44475a".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("tab.active".to_string(), ColorDef {
            fg: Some("#282a36".to_string()), bg: Some("#bd93f9".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("tab.inactive".to_string(), ColorDef {
            fg: Some("#6272a4".to_string()), bg: Some("#44475a".to_string()),
            bold: None, italic: None, underline: None,
        });
        Theme { name: "dracula".into(), author: Some("Tanu".into()), colors }
    }

    fn solarized() -> Theme {
        let mut colors = HashMap::new();
        colors.insert("background".to_string(), ColorDef {
            fg: Some("#839496".to_string()), bg: Some("#002b36".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("statusbar".to_string(), ColorDef {
            fg: Some("#002b36".to_string()), bg: Some("#268bd2".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("progressbar".to_string(), ColorDef {
            fg: Some("#002b36".to_string()), bg: Some("#859900".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.selected".to_string(), ColorDef {
            fg: Some("#002b36".to_string()), bg: Some("#268bd2".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("list.highlight".to_string(), ColorDef {
            fg: Some("#2aa198".to_string()), bg: None,
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("divider".to_string(), ColorDef {
            fg: Some("#073642".to_string()), bg: None,
            bold: None, italic: None, underline: None,
        });
        colors.insert("command_bar".to_string(), ColorDef {
            fg: Some("#839496".to_string()), bg: Some("#073642".to_string()),
            bold: None, italic: None, underline: None,
        });
        colors.insert("tab.active".to_string(), ColorDef {
            fg: Some("#002b36".to_string()), bg: Some("#d33682".to_string()),
            bold: Some(true), italic: None, underline: None,
        });
        colors.insert("tab.inactive".to_string(), ColorDef {
            fg: Some("#586e75".to_string()), bg: Some("#073642".to_string()),
            bold: None, italic: None, underline: None,
        });
        Theme { name: "solarized".into(), author: Some("Tanu".into()), colors }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_color("#00ff00"), Some(Color::Rgb(0, 255, 0)));
        assert_eq!(parse_color("#0000ff"), Some(Color::Rgb(0, 0, 255)));
    }

    #[test]
    fn test_parse_named_color() {
        assert_eq!(parse_color("red"), Some(Color::Red));
        assert_eq!(parse_color("white"), Some(Color::White));
    }

    #[test]
    fn test_parse_invalid_color() {
        assert_eq!(parse_color("notacolor"), None);
        assert_eq!(parse_color("#xyz"), None);
    }

    #[test]
    fn test_theme_switch() {
        let mut registry = ThemeRegistry::new();
        assert_eq!(registry.current_name(), "catppuccin-mocha");
        // Switching back to same theme should work
        assert!(registry.switch("catppuccin-mocha").is_ok());
    }

    #[test]
    fn test_all_builtin_themes() {
        let themes = Theme::all_builtin();
        assert_eq!(themes.len(), 8);
        assert!(themes.iter().any(|t| t.name == "catppuccin-mocha"));
        assert!(themes.iter().any(|t| t.name == "solarized"));
    }

    #[test]
    fn test_default_theme_has_entries() {
        let theme = Theme::catppuccin_mocha();
        assert!(!theme.colors.is_empty());
        assert!(theme.colors.contains_key("background"));
        assert!(theme.colors.contains_key("statusbar"));
    }

    #[test]
    fn test_theme_builtin_lookup() {
        assert!(Theme::builtin("gruvbox-dark").is_some());
        assert!(Theme::builtin("nord").is_some());
        assert!(Theme::builtin("tokyonight").is_some());
        assert!(Theme::builtin("nonexistent").is_none());
    }

    #[test]
    fn test_theme_preview() {
        let mut registry = ThemeRegistry::new();
        assert_eq!(registry.current_name(), "catppuccin-mocha");
        assert!(registry.preview_theme("dracula").is_ok());
        assert!(registry.has_preview());
        // Style should come from preview, current should stay
        let style = registry.style_for("background");
        assert!(style.fg.is_some());
        assert_eq!(registry.current_name(), "catppuccin-mocha");
        // Apply preview
        registry.apply_preview();
        assert!(!registry.has_preview());
        assert_eq!(registry.current_name(), "dracula");
    }

    #[test]
    fn test_theme_cancel_preview() {
        let mut registry = ThemeRegistry::new();
        assert!(registry.preview_theme("nord").is_ok());
        registry.cancel_preview();
        assert!(!registry.has_preview());
        assert_eq!(registry.current_name(), "catppuccin-mocha");
    }

    #[test]
    fn test_every_builtin_has_colors() {
        for theme in Theme::all_builtin() {
            assert!(!theme.colors.is_empty(), "Theme {} has no colors", theme.name);
            assert!(theme.colors.contains_key("background"), "Theme {} missing background", theme.name);
            assert!(theme.colors.contains_key("statusbar"), "Theme {} missing statusbar", theme.name);
            assert!(theme.colors.contains_key("tab.active"), "Theme {} missing tab.active", theme.name);
        }
    }

    #[test]
    fn test_all_theme_names_are_unique() {
        let themes = Theme::all_builtin();
        let mut names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
        names.sort();
        let len_before = names.len();
        names.dedup();
        assert_eq!(len_before, names.len(), "Theme names are not unique");
    }

    #[test]
    fn test_list_theme_names() {
        let registry = ThemeRegistry::new();
        let names = registry.list_names();
        assert!(names.contains(&"dracula"));
        assert!(names.contains(&"solarized"));
        assert_eq!(names.len(), 8);
    }

    #[test]
    fn test_color_def_to_style() {
        let def = ColorDef {
            fg: Some("#ff0000".to_string()),
            bg: Some("#000000".to_string()),
            bold: Some(true),
            italic: None,
            underline: None,
        };
        let style = def.to_style();
        assert_eq!(style.fg, Some(Color::Rgb(255, 0, 0)));
        assert_eq!(style.bg, Some(Color::Rgb(0, 0, 0)));
    }
}
