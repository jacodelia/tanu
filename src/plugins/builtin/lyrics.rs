//! Lyrics plugin.
//!
//! Provides synchronized and plain-text lyrics for the current track.
//! Searches in order:
//! 1. Local `.lrc` / `.txt` files next to the audio file
//! 2. lrclib.net API (requires `http-plugins` feature)
//!
//! Lyrics are emitted as events that the UI can display via popup or
//! a dedicated lyrics panel.
//!
//! Commands exposed:
//! - `:lyrics` — Show lyrics for the current track
//! - `:lyrics search <query>` — Search lrclib for lyrics

use std::path::{Path, PathBuf};

use crate::events::Event;
use crate::plugins::{Plugin, PluginContext};

/// A set of lyrics with optional timestamps.
#[derive(Debug, Clone)]
pub struct LyricsResult {
    pub track: String,
    pub artist: String,
    pub lines: Vec<LyricsLine>,
    pub source: LyricsSource,
}

/// A single line of lyrics (with optional sync timestamp).
#[derive(Debug, Clone)]
pub struct LyricsLine {
    pub timestamp_ms: Option<u64>,
    pub text: String,
}

/// Where the lyrics were found.
#[derive(Debug, Clone, PartialEq)]
pub enum LyricsSource {
    LocalFile(PathBuf),
    Lrclib,
    NotFound,
}

/// Lyrics plugin.
pub struct LyricsPlugin {
    /// Currently loaded lyrics.
    current: Option<LyricsResult>,
    /// Track path for which lyrics are loaded.
    current_track: Option<String>,
    /// Last search query.
    last_search: Option<String>,
}

impl LyricsPlugin {
    pub fn new() -> Self {
        Self {
            current: None,
            current_track: None,
            last_search: None,
        }
    }

    /// Try to load lyrics for a track.
    fn load_lyrics(
        &mut self,
        ctx: &PluginContext,
        artist: &str,
        title: &str,
        audio_path: Option<&Path>,
    ) {
        // 1. Try local .lrc file
        if let Some(path) = audio_path {
            if let Some(lyrics) = Self::find_local_lyrics(path, artist, title) {
                self.current = Some(lyrics);
                self.emit_result(ctx);
                return;
            }
        }

        // 2. Try lrclib API
        #[cfg(feature = "http-plugins")]
        {
            if let Some(lyrics) = Self::fetch_lrclib(artist, title) {
                self.current = Some(lyrics);
                self.emit_result(ctx);
                return;
            }
        }

        // 3. Not found
        self.current = Some(LyricsResult {
            track: title.to_string(),
            artist: artist.to_string(),
            lines: vec![LyricsLine {
                timestamp_ms: None,
                text: "No lyrics found.".to_string(),
            }],
            source: LyricsSource::NotFound,
        });
        self.emit_result(ctx);
    }

    /// Search for `.lrc` and `.txt` files next to the audio file.
    fn find_local_lyrics(audio_path: &Path, _artist: &str, _title: &str) -> Option<LyricsResult> {
        let parent = audio_path.parent()?;
        let stem = audio_path.file_stem()?.to_str()?;

        for ext in &["lrc", "txt"] {
            let candidate = parent.join(format!("{}.{}", stem, ext));
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                return Some(Self::parse_lrc(
                    &content,
                    _artist,
                    _title,
                    LyricsSource::LocalFile(candidate),
                ));
            }
        }

        None
    }

    /// Fetch lyrics from lrclib.net.
    #[cfg(feature = "http-plugins")]
    fn fetch_lrclib(artist: &str, title: &str) -> Option<LyricsResult> {
        let artist_enc = artist.replace(' ', "%20").replace('&', "%26");
        let title_enc = title.replace(' ', "%20").replace('&', "%26");
        let url = format!(
            "https://lrclib.net/api/get?artist_name={}&track_name={}",
            artist_enc, title_enc
        );

        let client = reqwest::blocking::Client::new();
        let response = client
            .get(&url)
            .header("User-Agent", "Tanu/0.1.0")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .ok()?;

        let json: serde_json::Value = response.json().ok()?;
        let synced = json.get("syncedLyrics").and_then(|v| v.as_str());
        let plain = json.get("plainLyrics").and_then(|v| v.as_str());

        let text = synced.or(plain)?;
        Some(Self::parse_lrc(text, artist, title, LyricsSource::Lrclib))
    }

    /// Fallback when http-plugins is not enabled.
    #[cfg(not(feature = "http-plugins"))]
    fn fetch_lrclib(_artist: &str, _title: &str) -> Option<LyricsResult> {
        None
    }

    /// Parse LRC (or plain text) content into lines.
    fn parse_lrc(content: &str, artist: &str, title: &str, source: LyricsSource) -> LyricsResult {
        let mut lines = Vec::new();
        let re = regex::Regex::new(r"^\[(\d{2}):(\d{2})(?:\.(\d{2,3}))?\]\s*(.*)").ok();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(ref re) = re {
                if let Some(caps) = re.captures(line) {
                    let min: u64 = caps
                        .get(1)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                    let sec: u64 = caps
                        .get(2)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                    let ms_str = caps.get(3).map(|m| m.as_str()).unwrap_or("0");
                    let ms: u64 = ms_str.parse().unwrap_or(0);
                    let text = caps
                        .get(4)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default();

                    let timestamp_ms = Some(
                        min * 60000 + sec * 1000 + if ms_str.len() == 2 { ms * 10 } else { ms },
                    );

                    lines.push(LyricsLine { timestamp_ms, text });
                    continue;
                }
            }

            // Plain text line
            lines.push(LyricsLine {
                timestamp_ms: None,
                text: line.to_string(),
            });
        }

        LyricsResult {
            track: title.to_string(),
            artist: artist.to_string(),
            lines,
            source,
        }
    }

    /// Emit the lyrics result as an event.
    fn emit_result(&self, ctx: &PluginContext) {
        if let Some(ref lyrics) = self.current {
            let formatted: String = lyrics
                .lines
                .iter()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
                .join("\n");

            ctx.emit(Event::Command(format!(
                "show_lyrics:{} - {}|{}|{}",
                lyrics.artist,
                lyrics.track,
                match lyrics.source {
                    LyricsSource::LocalFile(_) => "local",
                    LyricsSource::Lrclib => "lrclib",
                    LyricsSource::NotFound => "none",
                },
                formatted.replace('\n', "\\n")
            )));
        }
    }
}

impl Plugin for LyricsPlugin {
    fn name(&self) -> &str {
        "lyrics"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn author(&self) -> &str {
        "Tanu"
    }

    fn description(&self) -> &str {
        "Fetches synchronized and plain-text lyrics from local files and lrclib.net"
    }

    fn on_init(&mut self, _ctx: &PluginContext) {
        tracing::info!("Lyrics plugin initialized");
    }

    fn on_event(&mut self, ctx: &PluginContext, event: &Event) -> bool {
        match event {
            Event::Command(cmd) if cmd == "lyrics" || cmd == ":lyrics" => {
                if self.current.is_some() {
                    // Re-emit current lyrics
                    self.emit_result(ctx);
                } else {
                    ctx.emit(Event::Command(
                        "show_lyrics:No track|No lyrics available|none|Nothing playing. Start playback to fetch lyrics."
                            .into(),
                    ));
                }
                true
            }
            Event::Command(cmd)
                if cmd.starts_with("lyrics search ") || cmd.starts_with(":lyrics search ") =>
            {
                let query = cmd
                    .strip_prefix("lyrics search ")
                    .or_else(|| cmd.strip_prefix(":lyrics search "))
                    .unwrap_or("");

                self.last_search = Some(query.to_string());
                let parts: Vec<&str> = query.splitn(2, " - ").collect();
                let artist = parts[0];
                let title = parts.get(1).copied().unwrap_or("");
                self.load_lyrics(ctx, artist, title, None);
                true
            }
            Event::PlayerStateChanged(state) => {
                // Track changed? Load new lyrics
                let track_id_str = state
                    .track_id
                    .map(|id| format!("{:?}", id))
                    .unwrap_or_default();
                if self.current_track.as_deref() != Some(&track_id_str) {
                    self.current_track = Some(track_id_str.clone());
                    // In a full implementation, we'd look up artist+title from DB here
                    // For now, track ID suffices for caching
                }
                true
            }
            _ => false,
        }
    }
}

impl Default for LyricsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lrc_basic() {
        let content = "[00:12.00] Hello world\n[00:15.50] This is a test";
        let result = LyricsPlugin::parse_lrc(content, "Artist", "Title", LyricsSource::Lrclib);
        assert_eq!(result.lines.len(), 2);
        assert_eq!(result.lines[0].text, "Hello world");
        assert_eq!(result.lines[0].timestamp_ms, Some(12000));
        assert_eq!(result.lines[1].timestamp_ms, Some(15500));
    }

    #[test]
    fn test_parse_lrc_plain_text() {
        let content = "Just a line\nAnother line\nNo timestamps here";
        let result = LyricsPlugin::parse_lrc(
            content,
            "A",
            "T",
            LyricsSource::LocalFile(PathBuf::from("/tmp/test.lrc")),
        );
        assert_eq!(result.lines.len(), 3);
        assert!(result.lines.iter().all(|l| l.timestamp_ms.is_none()));
    }

    #[test]
    fn test_parse_lrc_mixed() {
        let content = "[00:01.00] Synced line\nPlain line";
        let result = LyricsPlugin::parse_lrc(content, "A", "T", LyricsSource::Lrclib);
        assert_eq!(result.lines.len(), 2);
        assert_eq!(result.lines[0].timestamp_ms, Some(1000));
        assert!(result.lines[1].timestamp_ms.is_none());
    }

    #[test]
    fn test_parse_lrc_minutes_seconds_only() {
        let content = "[01:30] No milliseconds\n[02:45] Another";
        let result = LyricsPlugin::parse_lrc(content, "A", "T", LyricsSource::Lrclib);
        assert_eq!(result.lines[0].timestamp_ms, Some(90000)); // 1:30 = 90,000ms
        assert_eq!(result.lines[1].timestamp_ms, Some(165000)); // 2:45 = 165,000ms
    }
}
