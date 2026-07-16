//! 10-band graphic equalizer (Winamp-style) with real audio processing.
//!
//! Band gains live in a shared [`EqState`] that the UI writes and the audio
//! thread reads. [`EqSource`] wraps the sample stream and applies ten cascaded
//! RBJ peaking biquads per channel, so the sliders actually change the sound.
//! Band frequencies match Winamp's classic EQ.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rodio::Source;

pub const EQ_BANDS: usize = 10;
/// Winamp classic center frequencies (Hz).
pub const EQ_FREQS: [f32; EQ_BANDS] =
    [70.0, 180.0, 320.0, 600.0, 1000.0, 3000.0, 6000.0, 12000.0, 14000.0, 16000.0];
/// Gain range of each slider, in dB.
pub const EQ_MAX_DB: f32 = 12.0;

struct Inner {
    gains_db: [f32; EQ_BANDS],
    preamp_db: f32,
    enabled: bool,
}

/// Cloneable handle to the shared EQ settings.
#[derive(Clone)]
pub struct EqState {
    inner: Arc<Mutex<Inner>>,
    /// Bumped on any change so the audio thread recomputes coefficients cheaply.
    version: Arc<AtomicU64>,
}

impl Default for EqState {
    fn default() -> Self {
        Self::new()
    }
}

impl EqState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                gains_db: [0.0; EQ_BANDS],
                preamp_db: 0.0,
                enabled: true,
            })),
            version: Arc::new(AtomicU64::new(1)),
        }
    }

    fn bump(&self) {
        self.version.fetch_add(1, Ordering::Release);
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    pub fn set_gain(&self, band: usize, db: f32) {
        if band < EQ_BANDS {
            self.inner.lock().unwrap().gains_db[band] = db.clamp(-EQ_MAX_DB, EQ_MAX_DB);
            self.bump();
        }
    }

    pub fn adjust_gain(&self, band: usize, delta: f32) {
        if band < EQ_BANDS {
            let mut g = self.inner.lock().unwrap();
            g.gains_db[band] = (g.gains_db[band] + delta).clamp(-EQ_MAX_DB, EQ_MAX_DB);
            drop(g);
            self.bump();
        }
    }

    pub fn set_all(&self, gains: [f32; EQ_BANDS], preamp: f32) {
        let mut g = self.inner.lock().unwrap();
        for (i, v) in gains.iter().enumerate() {
            g.gains_db[i] = v.clamp(-EQ_MAX_DB, EQ_MAX_DB);
        }
        g.preamp_db = preamp.clamp(-EQ_MAX_DB, EQ_MAX_DB);
        drop(g);
        self.bump();
    }

    pub fn set_preamp(&self, db: f32) {
        self.inner.lock().unwrap().preamp_db = db.clamp(-EQ_MAX_DB, EQ_MAX_DB);
        self.bump();
    }

    pub fn toggle_enabled(&self) {
        let mut g = self.inner.lock().unwrap();
        g.enabled = !g.enabled;
        drop(g);
        self.bump();
    }

    pub fn snapshot(&self) -> ([f32; EQ_BANDS], f32, bool) {
        let g = self.inner.lock().unwrap();
        (g.gains_db, g.preamp_db, g.enabled)
    }
}

/// A single RBJ peaking biquad (transposed direct form II).
#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn identity() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }

    /// Peaking EQ coefficients (RBJ cookbook), keeping filter state.
    fn set_peaking(&mut self, freq: f32, gain_db: f32, q: f32, fs: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * (freq / fs).min(0.49);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        self.b0 = (1.0 + alpha * a) / a0;
        self.b1 = (-2.0 * cos) / a0;
        self.b2 = (1.0 - alpha * a) / a0;
        self.a1 = (-2.0 * cos) / a0;
        self.a2 = (1.0 - alpha / a) / a0;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Wraps an `i16` source and applies the 10-band EQ per channel.
pub struct EqSource<S: Source<Item = i16>> {
    inner: S,
    eq: EqState,
    fs: f32,
    channels: usize,
    filters: Vec<[Biquad; EQ_BANDS]>, // per channel
    preamp_lin: f32,
    enabled: bool,
    seen_version: u64,
    ch: usize,
}

impl<S: Source<Item = i16>> EqSource<S> {
    pub fn new(inner: S, eq: EqState) -> Self {
        let fs = inner.sample_rate() as f32;
        let channels = inner.channels().max(1) as usize;
        let mut src = Self {
            inner,
            eq,
            fs,
            channels,
            filters: vec![[Biquad::identity(); EQ_BANDS]; channels],
            preamp_lin: 1.0,
            enabled: true,
            seen_version: 0,
            ch: 0,
        };
        src.recompute();
        src
    }

    fn recompute(&mut self) {
        let (gains, preamp, enabled) = self.eq.snapshot();
        self.enabled = enabled;
        self.preamp_lin = 10f32.powf(preamp / 20.0);
        for chan in self.filters.iter_mut() {
            for (i, bq) in chan.iter_mut().enumerate() {
                let keep = (bq.z1, bq.z2);
                bq.set_peaking(EQ_FREQS[i], gains[i], 1.0, self.fs);
                bq.z1 = keep.0;
                bq.z2 = keep.1;
            }
        }
        self.seen_version = self.eq.version();
    }
}

impl<S: Source<Item = i16>> Iterator for EqSource<S> {
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        let s = self.inner.next()?;
        if self.eq.version() != self.seen_version {
            self.recompute();
        }
        if !self.enabled {
            return Some(s);
        }
        let ch = self.ch;
        self.ch = (self.ch + 1) % self.channels;

        let mut x = (s as f32 / i16::MAX as f32) * self.preamp_lin;
        for bq in self.filters[ch].iter_mut() {
            x = bq.process(x);
        }
        let out = (x.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        Some(out)
    }
}

impl<S: Source<Item = i16>> Source for EqSource<S> {
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

/// Winamp-style presets (name, 10 band gains in dB). `preamp` omitted (0 dB).
pub const PRESETS: &[(&str, [f32; EQ_BANDS])] = &[
    ("Flat", [0.0; EQ_BANDS]),
    ("Rock", [4.6, 2.7, -3.9, -5.4, -2.7, 1.2, 5.4, 6.6, 6.6, 6.6]),
    ("Pop", [-1.5, 2.7, 4.2, 4.6, 3.1, -0.7, -1.5, -1.5, -1.5, -1.5]),
    ("Jazz", [3.9, 1.9, 0.7, 2.7, -1.9, -1.9, 0.0, 1.2, 3.9, 3.9]),
    ("Classical", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -5.4, -5.4, -5.4, -7.3]),
    ("Bass", [7.0, 6.0, 4.5, 2.5, 0.5, -1.0, -2.0, -2.5, -2.5, -2.5]),
    ("Treble", [-3.0, -3.0, -2.5, -1.0, 0.5, 2.5, 5.0, 7.0, 8.0, 8.0]),
    ("Vocal", [-3.0, -2.0, 0.5, 3.0, 4.5, 4.5, 3.5, 1.5, -1.0, -2.5]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gain_clamped_and_versioned() {
        let eq = EqState::new();
        let v0 = eq.version();
        eq.set_gain(0, 100.0);
        assert_eq!(eq.snapshot().0[0], EQ_MAX_DB);
        assert!(eq.version() > v0);
        eq.adjust_gain(0, -50.0);
        assert_eq!(eq.snapshot().0[0], -EQ_MAX_DB);
    }

    #[test]
    fn test_flat_eq_is_passthrough() {
        // Flat gains → identity-ish; a peaking filter at 0 dB is unity.
        let mut bq = Biquad::identity();
        bq.set_peaking(1000.0, 0.0, 1.0, 44100.0);
        let y: f32 = (0..64).map(|i| bq.process(if i == 0 { 1.0 } else { 0.0 })).map(|v| v.abs()).sum();
        // Impulse response energy ~1 (unity gain).
        assert!((y - 1.0).abs() < 0.05);
    }
}
