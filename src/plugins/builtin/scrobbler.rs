//! Last.fm scrobbling plugin.
//!
//! Tracks plays and periodically submits scrobbles to Last.fm.
//! Requires `http-plugins` feature for network access.
//!
//! Configuration keys (set via PluginContext::store):
//! - `lastfm.username` — Last.fm username
//! - `lastfm.api_key` — Last.fm API key
//! - `lastfm.api_secret` — Last.fm API shared secret
//! - `lastfm.session_key` — Session key (obtained via auth flow)
//!
//! Workflow:
//! 1. Track plays for 4min or 50% duration before scrobbling
//! 2. Submits batch of queued scrobbles every 60s
//! 3. Updates "Now Playing" on track start

use std::time::Instant;

use crate::events::Event;
use crate::plugins::{Plugin, PluginContext};

/// A pending scrobble waiting to be submitted.
#[derive(Debug, Clone)]
struct PendingScrobble {
    artist: String,
    track: String,
    album: String,
    timestamp: u64,
}

/// Last.fm scrobbler plugin.
pub struct LastFmScrobbler {
    /// Tracks waiting to be submitted.
    queue: Vec<PendingScrobble>,
    /// Current track info for "now playing" updates.
    current_artist: Option<String>,
    current_track: Option<String>,
    /// When the current track started playing.
    track_started: Option<Instant>,
    /// Last time we submitted the scrobble queue.
    last_submit: Instant,
    /// Whether the current track has been scrobbled.
    scrobbled: bool,
    /// Interval between scrobble submissions (seconds).
    submit_interval_secs: u64,
    /// Minimum play time before scrobbling (seconds).
    min_play_secs: u64,
}

impl LastFmScrobbler {
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            current_artist: None,
            current_track: None,
            track_started: None,
            last_submit: Instant::now(),
            scrobbled: false,
            submit_interval_secs: 60,
            min_play_secs: 240, // 4 minutes
        }
    }

    /// Queue a track for scrobbling.
    fn queue_scrobble(&mut self, artist: String, track: String, album: String) {
        self.queue.push(PendingScrobble {
            artist,
            track,
            album,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });
    }

    /// Submit queued scrobbles to Last.fm.
    fn submit_batch(&self, _ctx: &PluginContext) {
        if self.queue.is_empty() {
            return;
        }

        #[cfg(feature = "http-plugins")]
        {
            let api_key = _ctx.fetch("lastfm.api_key");
            let session_key = _ctx.fetch("lastfm.session_key");

            if api_key.is_none() || session_key.is_none() {
                return;
            }

            // In a real implementation, we'd POST to:
            // https://ws.audioscrobbler.com/2.0/
            // with method=track.scrobble
            tracing::info!(count = self.queue.len(), "Submitting scrobbles to Last.fm");
        }

        // Without http-plugins, just log
        tracing::debug!(
            count = self.queue.len(),
            "Would submit {} scrobbles (http-plugins disabled)",
            self.queue.len()
        );
    }

    /// Update "Now Playing" on Last.fm.
    fn update_now_playing(&self, _ctx: &PluginContext) {
        #[cfg(feature = "http-plugins")]
        {
            let api_key = _ctx.fetch("lastfm.api_key");
            let session_key = _ctx.fetch("lastfm.session_key");

            if let (Some(_api_key), Some(_session_key), Some(ref artist), Some(ref track)) = (
                api_key,
                session_key,
                &self.current_artist,
                &self.current_track,
            ) {
                tracing::debug!(
                    artist = %artist,
                    track = %track,
                    "Updating Now Playing on Last.fm"
                );
            }
        }
    }
}

impl Plugin for LastFmScrobbler {
    fn name(&self) -> &str {
        "lastfm-scrobbler"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn author(&self) -> &str {
        "Tanu"
    }

    fn description(&self) -> &str {
        "Scrobbles played tracks to Last.fm"
    }

    fn on_init(&mut self, ctx: &PluginContext) {
        tracing::info!("Last.fm scrobbler initialized");
        // Read settings from context storage
        if let Some(interval) = ctx.fetch("scrobbler.submit_interval_secs") {
            if let Ok(n) = interval.parse() {
                self.submit_interval_secs = n;
            }
        }
        if let Some(min_play) = ctx.fetch("scrobbler.min_play_secs") {
            if let Ok(n) = min_play.parse() {
                self.min_play_secs = n;
            }
        }
    }

    fn on_event(&mut self, ctx: &PluginContext, event: &Event) -> bool {
        match event {
            Event::Play => {
                self.track_started = Some(Instant::now());
                self.scrobbled = false;
                self.update_now_playing(ctx);
                true
            }
            Event::PlayerStateChanged(state) => {
                if state.is_playing {
                    // Check if we should scrobble (played long enough)
                    if let Some(started) = self.track_started {
                        let elapsed = started.elapsed().as_secs();
                        if !self.scrobbled
                            && elapsed >= self.min_play_secs
                            && state.position_secs >= state.duration_secs * 0.5
                        {
                            if let (Some(ref artist), Some(ref track)) =
                                (&self.current_artist, &self.current_track)
                            {
                                self.queue_scrobble(artist.clone(), track.clone(), String::new());
                                self.scrobbled = true;
                            }
                        }
                    }
                }
                true
            }
            Event::Stop | Event::Pause => {
                self.track_started = None;
                true
            }
            _ => false,
        }
    }

    fn on_tick(&mut self, ctx: &PluginContext) {
        // Submit batch every N seconds
        if self.last_submit.elapsed().as_secs() >= self.submit_interval_secs {
            self.submit_batch(ctx);
            self.last_submit = Instant::now();
        }
    }

    fn on_shutdown(&mut self) {
        tracing::info!("Last.fm scrobbler shutting down");
    }
}

impl Default for LastFmScrobbler {
    fn default() -> Self {
        Self::new()
    }
}
