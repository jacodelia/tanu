//! Configuration management.
//!
//! Reads TOML files, supports hot-reload via file watchers,
//! and exposes typed configuration structs.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub general: GeneralConfig,
    pub library: LibraryConfig,
    pub audio: AudioConfig,
    pub ui: UiConfig,
    pub keybindings: KeybindingsConfig,
}

/// General application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Path to the database file.
    pub database_path: PathBuf,
    /// Log file path. Empty string disables file logging.
    pub log_file: String,
    /// Log level: trace, debug, info, warn, error.
    pub log_level: String,
}

/// Library indexing settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryConfig {
    /// Directories to scan for music files.
    pub music_dirs: Vec<PathBuf>,
    /// File extensions to include.
    pub extensions: Vec<String>,
    /// Whether to watch for filesystem changes.
    pub watch_enabled: bool,
    /// Debounce delay for filesystem events in milliseconds.
    pub watch_debounce_ms: u64,
}

/// Audio playback settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Default volume (0.0 to 1.0).
    pub default_volume: f32,
    /// Whether replaygain is enabled.
    pub replaygain_enabled: bool,
    /// Whether gapless playback is enabled.
    pub gapless_enabled: bool,
    /// Crossfade duration in seconds.
    pub crossfade_secs: f64,
    /// Audio backend: "rodio" or "kira".
    pub backend: String,
}

/// User interface settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Theme name.
    pub theme: String,
    /// Primary/accent typography color as `#rrggbb` (EDIT → Text Color).
    #[serde(default)]
    pub text_color: Option<String>,
    /// UI language code (`en`, `it`, `fr`, ...). EDIT → Language.
    #[serde(default)]
    pub language: Option<String>,
    /// Mouse enabled.
    pub mouse_enabled: bool,
    /// Frame rate cap for rendering.
    pub max_fps: u32,
    /// Whether to show the status bar.
    pub show_status_bar: bool,
    /// Whether to show the progress bar.
    pub show_progress_bar: bool,
}

/// Key binding settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingsConfig {
    /// Path to the bindings configuration file.
    pub bindings_path: PathBuf,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            database_path: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("tanu")
                .join("tanu.db"),
            log_file: String::new(),
            log_level: "info".to_string(),
        }
    }
}

impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            music_dirs: vec![dirs::audio_dir().unwrap_or_else(|| PathBuf::from("~/Music"))],
            extensions: vec![
                "mp3".to_string(),
                "flac".to_string(),
                "ogg".to_string(),
                "opus".to_string(),
                "wav".to_string(),
                "m4a".to_string(),
                "aac".to_string(),
                "wma".to_string(),
            ],
            watch_enabled: true,
            watch_debounce_ms: 500,
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            default_volume: 0.8,
            replaygain_enabled: false,
            gapless_enabled: false,
            crossfade_secs: 0.0,
            backend: "rodio".to_string(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            text_color: None,
            language: None,
            mouse_enabled: true,
            max_fps: 60,
            show_status_bar: true,
            show_progress_bar: true,
        }
    }
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            bindings_path: dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("tanu")
                .join("bindings.toml"),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to a TOML file.
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load with defaults applied for missing values.
    pub fn load_or_default(path: &std::path::Path) -> Self {
        Self::load(path).unwrap_or_default()
    }
}

/// Key binding: maps a key sequence to an action in a given mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub action: String,
    pub description: Option<String>,
}

/// Full keybinding configuration per mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BindingsConfig {
    pub normal: Vec<KeyBinding>,
    pub insert: Vec<KeyBinding>,
    pub command: Vec<KeyBinding>,
    pub visual: Vec<KeyBinding>,
}

impl BindingsConfig {
    /// Load from a TOML file.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let bindings: BindingsConfig = toml::from_str(&content)?;
        Ok(bindings)
    }

    /// Generate default Vim-inspired bindings.
    pub fn default_bindings() -> Self {
        Self {
            normal: vec![
                KeyBinding {
                    key: "j".to_string(),
                    action: "scroll_down".to_string(),
                    description: Some("Scroll down".to_string()),
                },
                KeyBinding {
                    key: "k".to_string(),
                    action: "scroll_up".to_string(),
                    description: Some("Scroll up".to_string()),
                },
                KeyBinding {
                    key: "gg".to_string(),
                    action: "scroll_top".to_string(),
                    description: Some("Go to top".to_string()),
                },
                KeyBinding {
                    key: "G".to_string(),
                    action: "scroll_bottom".to_string(),
                    description: Some("Go to bottom".to_string()),
                },
                KeyBinding {
                    key: "/".to_string(),
                    action: "search".to_string(),
                    description: Some("Search forward".to_string()),
                },
                KeyBinding {
                    key: ":".to_string(),
                    action: "command_mode".to_string(),
                    description: Some("Enter command mode".to_string()),
                },
                KeyBinding {
                    key: "space".to_string(),
                    action: "smart_play_pause".to_string(),
                    description: Some("Play selected / Pause".to_string()),
                },
                KeyBinding {
                    key: "enter".to_string(),
                    action: "select".to_string(),
                    description: Some("Select / Open".to_string()),
                },
                KeyBinding {
                    key: "+".to_string(),
                    action: "volume_up".to_string(),
                    description: Some("Volume up".to_string()),
                },
                KeyBinding {
                    key: "=".to_string(),
                    action: "volume_up".to_string(),
                    description: Some("Volume up".to_string()),
                },
                KeyBinding {
                    key: "-".to_string(),
                    action: "volume_down".to_string(),
                    description: Some("Volume down".to_string()),
                },
                KeyBinding {
                    key: "tab".to_string(),
                    action: "next_view".to_string(),
                    description: Some("Next view".to_string()),
                },
                KeyBinding {
                    key: "shift+tab".to_string(),
                    action: "previous_view".to_string(),
                    description: Some("Previous view".to_string()),
                },
                KeyBinding {
                    key: "q".to_string(),
                    action: "quit".to_string(),
                    description: Some("Quit".to_string()),
                },
            ],
            insert: vec![KeyBinding {
                key: "escape".to_string(),
                action: "normal_mode".to_string(),
                description: Some("Return to normal mode".to_string()),
            }],
            command: vec![
                KeyBinding {
                    key: "escape".to_string(),
                    action: "normal_mode".to_string(),
                    description: Some("Return to normal mode".to_string()),
                },
                KeyBinding {
                    key: "enter".to_string(),
                    action: "execute_command".to_string(),
                    description: Some("Execute command".to_string()),
                },
            ],
            visual: vec![
                KeyBinding {
                    key: "escape".to_string(),
                    action: "normal_mode".to_string(),
                    description: Some("Return to normal mode".to_string()),
                },
                KeyBinding {
                    key: "y".to_string(),
                    action: "yank".to_string(),
                    description: Some("Copy selection".to_string()),
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.audio.default_volume, 0.8);
        assert!(config.ui.mouse_enabled);
        assert_eq!(config.general.log_level, "info");
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.audio.default_volume, config.audio.default_volume);
        assert_eq!(parsed.ui.theme, config.ui.theme);
    }

    #[test]
    fn test_default_bindings_has_normal_bindings() {
        let bindings = BindingsConfig::default_bindings();
        assert!(!bindings.normal.is_empty());
        assert!(bindings.normal.iter().any(|b| b.key == "j"));
        assert!(bindings.normal.iter().any(|b| b.key == "k"));
    }

    #[test]
    fn test_bindings_serialization() {
        let bindings = BindingsConfig::default_bindings();
        let toml_str = toml::to_string_pretty(&bindings).unwrap();
        let parsed: BindingsConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.normal.len(), bindings.normal.len());
    }
}
