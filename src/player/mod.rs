//! Audio playback engine.
//!
//! Decodes audio files using symphonia, outputs via rodio.
//! The player runs on a dedicated OS thread because `rodio::OutputStream`
//! is not `Send` on Linux (ALSA).
//!
//! Communication: `PlayerCommand` enum sent via `std::sync::mpsc`,
//! state updates emitted via tokio `EventSender`.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use crate::events::{Event, PlayerState, RepeatMode};

/// Commands sent to the player thread.
pub enum PlayerCommand {
    Play,
    Pause,
    Resume,
    Stop,
    TogglePlayPause,
    Next,
    Previous,
    Seek(f64),
    SetVolume(f32),
    SetShuffle(bool),
    SetRepeat(RepeatMode),
    Enqueue(PathBuf),
    ClearQueue,
    Quit,
}

/// Trait for audio backends (rodio, kira, etc.).
pub trait AudioBackend {
    fn play(&self, path: &Path) -> anyhow::Result<()>;
    fn pause(&self);
    fn resume(&self);
    fn stop(&self);
    fn seek(&self, position_secs: f64);
    fn set_volume(&self, volume: f32);
    fn position(&self) -> f64;
    fn duration(&self) -> f64;
    fn is_playing(&self) -> bool;
}

/// A stub backend for testing.
pub struct StubAudioBackend {
    playing: bool,
    position: f64,
    duration: f64,
    volume: f32,
}

impl StubAudioBackend {
    pub fn new() -> Self {
        Self {
            playing: false,
            position: 0.0,
            duration: 0.0,
            volume: 0.8,
        }
    }
}

impl AudioBackend for StubAudioBackend {
    fn play(&self, _path: &Path) -> anyhow::Result<()> {
        Ok(())
    }
    fn pause(&self) {}
    fn resume(&self) {}
    fn stop(&self) {}
    fn seek(&self, _position_secs: f64) {}
    fn set_volume(&self, _volume: f32) {}
    fn position(&self) -> f64 {
        self.position
    }
    fn duration(&self) -> f64 {
        self.duration
    }
    fn is_playing(&self) -> bool {
        self.playing
    }
}

/// The player engine: manages playback state, a path-based queue,
/// and the audio backend. Runs on a dedicated thread.
pub struct Player {
    backend: Box<dyn AudioBackend>,
    state: PlayerState,
    queue: Vec<PathBuf>,
    queue_position: usize,
    volume: f32,
}

impl Player {
    pub fn new(backend: Box<dyn AudioBackend>) -> Self {
        Self {
            backend,
            state: PlayerState {
                track_id: None,
                is_playing: false,
                position_secs: 0.0,
                duration_secs: 0.0,
                volume: 0.8,
                shuffle: false,
                repeat: RepeatMode::Off,
            },
            queue: Vec::new(),
            queue_position: 0,
            volume: 0.8,
        }
    }

    pub fn enqueue(&mut self, path: PathBuf) {
        self.queue.push(path);
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.queue_position = 0;
    }

    fn play_current(&mut self) -> anyhow::Result<()> {
        if let Some(path) = self.queue.get(self.queue_position) {
            self.backend.play(path)?;
            self.state.is_playing = true;
        }
        Ok(())
    }

    fn toggle_play_pause(&mut self) -> anyhow::Result<()> {
        if self.backend.is_playing() {
            self.backend.pause();
            self.state.is_playing = false;
        } else if self.queue.get(self.queue_position).is_some() {
            self.backend.resume();
            self.state.is_playing = true;
        } else if !self.queue.is_empty() {
            self.play_current()?;
        }
        Ok(())
    }

    fn pause(&mut self) {
        self.backend.pause();
        self.state.is_playing = false;
    }

    fn resume(&mut self) {
        self.backend.resume();
        self.state.is_playing = true;
    }

    fn stop(&mut self) {
        self.backend.stop();
        self.state.is_playing = false;
        self.state.position_secs = 0.0;
        self.state.track_id = None;
    }

    /// Detect a finished track (sink drained near the end) and advance
    /// according to the repeat mode: replay, next, wrap, or stop.
    fn check_track_end(&mut self) {
        if !self.state.is_playing {
            return;
        }
        let dur = self.backend.duration();
        let pos = self.backend.position();
        if dur <= 1.0 || pos < dur - 0.4 || self.backend.is_playing() {
            return;
        }
        match self.state.repeat {
            RepeatMode::Track => {
                let _ = self.play_current();
            }
            RepeatMode::Playlist => {
                self.queue_position = if self.queue_position + 1 < self.queue.len() {
                    self.queue_position + 1
                } else {
                    0
                };
                let _ = self.play_current();
            }
            RepeatMode::Off => {
                if self.queue_position + 1 < self.queue.len() {
                    self.queue_position += 1;
                    let _ = self.play_current();
                } else {
                    self.stop();
                }
            }
        }
    }

    fn next(&mut self) -> anyhow::Result<()> {
        if self.queue_position + 1 < self.queue.len() {
            self.queue_position += 1;
            self.play_current()?;
        }
        Ok(())
    }

    fn previous(&mut self) -> anyhow::Result<()> {
        if self.queue_position > 0 {
            self.queue_position -= 1;
            self.play_current()?;
        }
        Ok(())
    }

    fn set_volume(&mut self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        self.volume = clamped;
        self.state.volume = clamped;
        self.backend.set_volume(clamped);
    }

    fn set_shuffle(&mut self, enabled: bool) {
        self.state.shuffle = enabled;
    }

    fn set_repeat(&mut self, mode: RepeatMode) {
        self.state.repeat = mode;
    }

    fn seek(&mut self, position: f64) {
        self.state.position_secs = position;
        self.backend.seek(position);
    }

    fn current_state(&self) -> PlayerState {
        let pos = self.backend.position();
        let dur = self.backend.duration();
        let playing = self.backend.is_playing();

        PlayerState {
            position_secs: pos,
            duration_secs: dur,
            is_playing: playing,
            volume: self.volume,
            ..self.state
        }
    }

    /// Runs the player on the current thread. Blocks until `Quit` is received.
    pub fn run(
        mut self,
        cmd_rx: mpsc::Receiver<PlayerCommand>,
        event_tx: crate::events::bus::EventSender,
    ) {
        let tick = Duration::from_millis(500);

        loop {
            // Non-blocking recv with timeout for state ticks
            match cmd_rx.recv_timeout(tick) {
                Ok(PlayerCommand::Quit) => break,
                Ok(PlayerCommand::Play) => {
                    let _ = self.play_current();
                }
                Ok(PlayerCommand::Pause) => {
                    self.pause();
                }
                Ok(PlayerCommand::Resume) => {
                    self.resume();
                }
                Ok(PlayerCommand::Stop) => {
                    self.stop();
                }
                Ok(PlayerCommand::TogglePlayPause) => {
                    let _ = self.toggle_play_pause();
                }
                Ok(PlayerCommand::Next) => {
                    let _ = self.next();
                }
                Ok(PlayerCommand::Previous) => {
                    let _ = self.previous();
                }
                Ok(PlayerCommand::Seek(pos)) => {
                    self.seek(pos);
                }
                Ok(PlayerCommand::SetVolume(v)) => {
                    self.set_volume(v);
                }
                Ok(PlayerCommand::SetShuffle(enabled)) => {
                    self.set_shuffle(enabled);
                }
                Ok(PlayerCommand::SetRepeat(mode)) => {
                    self.set_repeat(mode);
                }
                Ok(PlayerCommand::Enqueue(path)) => {
                    self.enqueue(path);
                }
                Ok(PlayerCommand::ClearQueue) => {
                    self.clear_queue();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Tick: emit current state
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            self.check_track_end();

            let state = self.current_state();
            let _ = event_tx.send(Event::PlayerStateChanged(state));
        }

        tracing::info!("Player thread stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_player() -> Player {
        Player::new(Box::new(StubAudioBackend::new()))
    }

    #[test]
    fn test_player_default_state() {
        let player = test_player();
        let state = player.current_state();
        assert!(!state.is_playing);
        assert_eq!(state.volume, 0.8);
    }

    #[test]
    fn test_player_volume_clamped() {
        let mut player = test_player();
        player.set_volume(1.5);
        assert_eq!(player.current_state().volume, 1.0);
        player.set_volume(-0.5);
        assert_eq!(player.current_state().volume, 0.0);
    }

    #[test]
    fn test_queue_management() {
        let mut player = test_player();
        player.enqueue(PathBuf::from("/tmp/test.mp3"));
        assert_eq!(player.queue.len(), 1);
        player.clear_queue();
        assert!(player.queue.is_empty());
    }

    #[test]
    fn test_navigation() {
        let mut player = test_player();
        player.enqueue(PathBuf::from("/tmp/a.mp3"));
        player.enqueue(PathBuf::from("/tmp/b.mp3"));
        assert_eq!(player.queue_position, 0);
        assert!(player.next().is_ok());
        assert_eq!(player.queue_position, 1);
        assert!(player.previous().is_ok());
        assert_eq!(player.queue_position, 0);
    }
}
