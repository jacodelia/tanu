//! Discord Rich Presence plugin.
//!
//! Updates Discord's "Now Playing" status with the current track.
//! Requires `http-plugins` feature for network access.
//!
//! Uses Discord's IPC protocol to update rich presence.
//! In a full implementation, this would use a Discord RPC library.
//! Currently demonstrates the plugin API with event tracking.

use std::time::Instant;

use crate::events::Event;
use crate::plugins::{Plugin, PluginContext};

/// Represents the current Discord presence state.
#[derive(Debug, Clone)]
struct PresenceState {
    artist: String,
    track: String,
    album: String,
    is_playing: bool,
    position_secs: f64,
    duration_secs: f64,
    start_timestamp: u64,
}

/// Discord Rich Presence plugin.
pub struct DiscordPresence {
    /// Current presence state being shown.
    state: Option<PresenceState>,
    /// Last time we updated Discord.
    last_update: Instant,
    /// Minimum interval between updates (seconds).
    update_interval_secs: u64,
    /// Whether to show track timestamps.
    show_timestamps: bool,
}

impl DiscordPresence {
    pub fn new() -> Self {
        Self {
            state: None,
            last_update: Instant::now(),
            update_interval_secs: 15,
            show_timestamps: true,
        }
    }

    /// Update Discord rich presence via RPC.
    fn update_presence(&self, _ctx: &PluginContext) {
        let state = match &self.state {
            Some(s) => s,
            None => return,
        };

        #[cfg(feature = "http-plugins")]
        {
            // In a real implementation, we'd connect to Discord's
            // IPC pipe and send a SET_ACTIVITY command.
            tracing::debug!(
                artist = %state.artist,
                track = %state.track,
                playing = state.is_playing,
                "Updating Discord Rich Presence"
            );
        }

        tracing::debug!(
            artist = %state.artist,
            track = %state.track,
            "Would update Discord RPC (http-plugins disabled)"
        );
    }

    /// Clear Discord presence (nothing playing).
    fn clear_presence(&self, _ctx: &PluginContext) {
        #[cfg(feature = "http-plugins")]
        {
            tracing::debug!("Clearing Discord Rich Presence");
        }
        tracing::debug!("Would clear Discord RPC");
    }

    fn format_duration(secs: f64) -> String {
        let m = (secs / 60.0) as u64;
        let s = (secs % 60.0) as u64;
        format!("{}:{:02}", m, s)
    }
}

impl Plugin for DiscordPresence {
    fn name(&self) -> &str {
        "discord-presence"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn author(&self) -> &str {
        "Tanu"
    }

    fn description(&self) -> &str {
        "Shows current track in Discord Rich Presence"
    }

    fn on_init(&mut self, ctx: &PluginContext) {
        tracing::info!("Discord Rich Presence plugin initialized");
        if let Some(interval) = ctx.fetch("discord.update_interval_secs") {
            if let Ok(n) = interval.parse() {
                self.update_interval_secs = n;
            }
        }
    }

    fn on_event(&mut self, ctx: &PluginContext, event: &Event) -> bool {
        match event {
            Event::Play => {
                if let Some(ref mut state) = self.state {
                    state.is_playing = true;
                    state.start_timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                }
                self.update_presence(ctx);
                self.last_update = Instant::now();
                true
            }
            Event::Pause | Event::Stop => {
                if let Some(ref mut state) = self.state {
                    state.is_playing = false;
                }
                self.update_presence(ctx);
                true
            }
            Event::PlayerStateChanged(state_data) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let new_state = PresenceState {
                    artist: String::new(), // populated by external track metadata
                    track: format!(
                        "Track {}",
                        state_data
                            .track_id
                            .map(|id| format!("{:?}", id))
                            .unwrap_or_default()
                    ),
                    album: String::new(),
                    is_playing: state_data.is_playing,
                    position_secs: state_data.position_secs,
                    duration_secs: state_data.duration_secs,
                    start_timestamp: now,
                };

                self.state = Some(new_state);
                true
            }
            _ => false,
        }
    }

    fn on_tick(&mut self, ctx: &PluginContext) {
        if self.last_update.elapsed().as_secs() >= self.update_interval_secs {
            if let Some(ref state) = self.state {
                if state.is_playing {
                    self.update_presence(ctx);
                    self.last_update = Instant::now();
                }
            }
        }
    }

    fn on_shutdown(&mut self) {
        tracing::info!("Discord Rich Presence plugin shutting down");
    }
}

impl Default for DiscordPresence {
    fn default() -> Self {
        Self::new()
    }
}
