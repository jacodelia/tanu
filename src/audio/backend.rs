//! Rodio audio backend — implements `AudioBackend` via rodio.
//!
//! Uses a rodio `OutputStream` + `Sink` for playback.
//! Position tracking is manual (rodio Sink doesn't expose elapsed time).
//! Interior mutability via `parking_lot::Mutex` so the trait's `&self` methods
//! can mutate state.

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamHandle, Sink};

use super::decoder;
use crate::player::AudioBackend;

/// Inner state behind a Mutex for interior mutability.
struct RodioInner {
    sink: Option<Sink>,
    current_duration: f64,
    volume: f32,
    playing: bool,
    paused: bool,
    started_at: Option<Instant>,
    elapsed_before_pause: f64,
}

/// The rodio-based audio backend.
pub struct RodioBackend {
    #[allow(dead_code)]
    stream: OutputStream,
    handle: OutputStreamHandle,
    inner: Mutex<RodioInner>,
}

impl RodioBackend {
    pub fn new() -> anyhow::Result<Self> {
        let (stream, handle) = OutputStream::try_default()?;
        Ok(Self {
            stream,
            handle,
            inner: Mutex::new(RodioInner {
                sink: None,
                current_duration: 0.0,
                volume: 0.8,
                playing: false,
                paused: false,
                started_at: None,
                elapsed_before_pause: 0.0,
            }),
        })
    }
}

impl AudioBackend for RodioBackend {
    fn play(&self, path: &Path) -> anyhow::Result<()> {
        let decoded = decoder::decode_file(path)?;

        let source = SamplesBuffer::new(
            decoded.channels,
            decoded.sample_rate,
            decoded.samples,
        );

        let sink = Sink::try_new(&self.handle)?;
        sink.set_volume(self.inner.lock().unwrap().volume);
        sink.append(source);

        let mut inner = self.inner.lock().unwrap();

        // Drop old sink (stops previous playback)
        inner.sink = Some(sink);
        inner.current_duration = decoded.duration_secs;
        inner.playing = true;
        inner.paused = false;
        inner.started_at = Some(Instant::now());
        inner.elapsed_before_pause = 0.0;

        Ok(())
    }

    fn pause(&self) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ref sink) = inner.sink {
            if inner.playing && !inner.paused {
                sink.pause();
                inner.paused = true;

                // Record elapsed time
                if let Some(start) = inner.started_at {
                    inner.elapsed_before_pause += start.elapsed().as_secs_f64();
                }
                inner.started_at = None;
            }
        }
    }

    fn resume(&self) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ref sink) = inner.sink {
            if inner.paused {
                sink.play();
                inner.paused = false;
                inner.started_at = Some(Instant::now());
            }
        }
    }

    fn stop(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.sink = None;
        inner.playing = false;
        inner.paused = false;
        inner.started_at = None;
        inner.elapsed_before_pause = 0.0;
        inner.current_duration = 0.0;
    }

    fn seek(&self, position_secs: f64) {
        let _ = position_secs;
        // Rodio Sink doesn't support seeking natively.
        // Full seek requires re-decoding from the target position
        // and re-appending. Stubbed for now.
    }

    fn set_volume(&self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        let mut inner = self.inner.lock().unwrap();
        inner.volume = clamped;
        if let Some(ref sink) = inner.sink {
            sink.set_volume(clamped);
        }
    }

    fn position(&self) -> f64 {
        let inner = self.inner.lock().unwrap();
        if inner.playing && !inner.paused {
            if let Some(start) = inner.started_at {
                let elapsed = inner.elapsed_before_pause + start.elapsed().as_secs_f64();
                return elapsed.min(inner.current_duration);
            }
        }
        inner.elapsed_before_pause.min(inner.current_duration)
    }

    fn duration(&self) -> f64 {
        self.inner.lock().unwrap().current_duration
    }

    fn is_playing(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.playing
            && !inner.paused
            && inner
                .sink
                .as_ref()
                .map(|s| !s.empty())
                .unwrap_or(false)
    }
}
