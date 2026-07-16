//! Shared audio visualization buffer.
//!
//! Playback streams lazily through rodio (instant start). A [`TappedSource`]
//! wraps the decoder and pushes a decimated mono copy of the samples — as they
//! actually play — into a small ring buffer. The oscilloscope reads the most
//! recent window, so the trace is the real audio in real time.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use rodio::Source;

/// One out of every `DECIMATE` frames is kept (downmixed to mono).
const DECIMATE: usize = 8;
/// Ring capacity in decimated samples (~a few hundred ms at 44.1kHz/8).
const RING_CAP: usize = 1024;

#[derive(Default)]
struct Inner {
    ring: VecDeque<f32>,
    active: bool,
    /// Effective sample rate of the ring (original rate / DECIMATE).
    rate: f64,
}

/// Cloneable handle to the shared visualization state.
#[derive(Clone, Default)]
pub struct AudioViz(Arc<Mutex<Inner>>);

impl AudioViz {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Inner::default())))
    }

    /// Mark a new track playing and clear the ring.
    pub fn on_play(&self) {
        let mut g = self.0.lock().unwrap();
        g.ring.clear();
        g.active = true;
    }

    pub fn set_active(&self, active: bool) {
        self.0.lock().unwrap().active = active;
    }

    /// Set the effective sample rate of the ring buffer.
    pub fn set_rate(&self, rate: f64) {
        self.0.lock().unwrap().rate = rate;
    }

    pub fn rate(&self) -> f64 {
        self.0.lock().unwrap().rate
    }

    /// Snapshot of the current ring samples (for spectrum analysis).
    pub fn raw_window(&self) -> Vec<f32> {
        self.0.lock().unwrap().ring.iter().copied().collect()
    }

    pub fn on_stop(&self) {
        let mut g = self.0.lock().unwrap();
        g.ring.clear();
        g.active = false;
    }

    /// Push one decimated mono sample (called from the audio thread).
    fn push(&self, s: f32) {
        let mut g = self.0.lock().unwrap();
        if g.ring.len() >= RING_CAP {
            g.ring.pop_front();
        }
        g.ring.push_back(s);
    }

    /// Test-only: push a sample directly.
    #[cfg(test)]
    pub fn push_test(&self, s: f32) {
        self.push(s);
    }

    /// Is a track actively playing (not paused/stopped) with data to show?
    pub fn is_active(&self) -> bool {
        let g = self.0.lock().unwrap();
        g.active && !g.ring.is_empty()
    }

    /// `n` waveform points in [-1, 1] from the most recent samples.
    pub fn waveform(&self, n: usize) -> Vec<f32> {
        let g = self.0.lock().unwrap();
        if g.ring.is_empty() || n == 0 {
            return Vec::new();
        }
        let len = g.ring.len();
        let step = len as f64 / n as f64;
        (0..n)
            .map(|i| {
                let idx = ((i as f64) * step) as usize;
                g.ring.get(idx.min(len - 1)).copied().unwrap_or(0.0)
            })
            .collect()
    }
}

/// A rodio `Source` adapter that passes samples through unchanged while
/// tapping a decimated mono copy into an [`AudioViz`] ring.
pub struct TappedSource<S>
where
    S: Source<Item = i16>,
{
    inner: S,
    viz: AudioViz,
    channels: usize,
    frame_sum: f32,
    frame_count: usize,
    frame_index: usize,
}

impl<S> TappedSource<S>
where
    S: Source<Item = i16>,
{
    pub fn new(inner: S, viz: AudioViz) -> Self {
        let channels = inner.channels().max(1) as usize;
        viz.set_rate(inner.sample_rate() as f64 / DECIMATE as f64);
        Self {
            inner,
            viz,
            channels,
            frame_sum: 0.0,
            frame_count: 0,
            frame_index: 0,
        }
    }
}

impl<S> Iterator for TappedSource<S>
where
    S: Source<Item = i16>,
{
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        let s = self.inner.next()?;
        // Accumulate a frame across channels, then decimate to mono.
        self.frame_sum += s as f32 / i16::MAX as f32;
        self.frame_count += 1;
        if self.frame_count >= self.channels {
            let mono = self.frame_sum / self.channels as f32;
            self.frame_sum = 0.0;
            self.frame_count = 0;
            if self.frame_index % DECIMATE == 0 {
                self.viz.push(mono);
            }
            self.frame_index += 1;
        }
        Some(s)
    }
}

impl<S> Source for TappedSource<S>
where
    S: Source<Item = i16>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_fills_and_reads() {
        let viz = AudioViz::new();
        viz.on_play();
        for _ in 0..2000 {
            viz.push(0.5);
        }
        assert!(viz.is_active());
        let wf = viz.waveform(64);
        assert_eq!(wf.len(), 64);
        assert!(wf.iter().all(|&s| (s - 0.5).abs() < 1e-6));
        viz.on_stop();
        assert!(viz.waveform(64).is_empty());
        assert!(!viz.is_active());
    }
}
