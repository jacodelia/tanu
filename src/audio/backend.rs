//! Rodio audio backend — implements `AudioBackend` via rodio.
//!
//! Uses a rodio `OutputStream` + `Sink` for playback.
//! Position tracking is manual (rodio Sink doesn't expose elapsed time).
//! Interior mutability via `parking_lot::Mutex` so the trait's `&self` methods
//! can mutate state.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// User-selected SoundFont, shared with the app (EDIT → SoundFont). `None`
/// falls back to auto-detection.
pub type SharedSoundFont = Arc<Mutex<Option<PathBuf>>>;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

use super::eq::{EqSource, EqState};
use super::viz::{AudioViz, TappedSource};
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

/// Render a MIDI file to a temporary WAV via fluidsynth (offline, fast-render),
/// then open it and unlink immediately (the fd keeps the data alive), so it can
/// be played through the normal rodio pipeline — giving seek, EQ, and the scope.
fn render_midi_to_pcm(sf2: &Path, midi: &Path, gain: f32) -> anyhow::Result<File> {
    let tmp = std::env::temp_dir().join(format!(
        "tanu-midi-{}-{}.wav",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    let status = Command::new("fluidsynth")
        .arg("-ni") // no midi-in, no interactive shell
        .arg("-q")
        .arg("-g")
        .arg(format!("{:.2}", gain.clamp(0.0, 10.0)))
        .arg("-r")
        .arg("44100")
        .arg("-F")
        .arg(&tmp) // fast-render to this WAV, then exit
        .arg(sf2)
        .arg(midi)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to launch fluidsynth: {e}"))?;
    if !status.success() || !tmp.exists() {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!("fluidsynth failed to render MIDI");
    }
    let file = File::open(&tmp)?;
    let _ = std::fs::remove_file(&tmp); // unlink; fd keeps it readable
    Ok(file)
}

// (Legacy MIDI parser kept below only for the duration unit test.)
#[cfg(test)]
struct MidiInfo {
    ppq: u32,
    tempo_us: u32,
    duration_secs: f64,
}

#[cfg(test)]
fn parse_midi(path: &Path) -> Option<MidiInfo> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 14 || &data[0..4] != b"MThd" {
        return None;
    }
    let division = u16::from_be_bytes([data[12], data[13]]);
    let ppq = if division & 0x8000 != 0 {
        480
    } else {
        (division as u32).max(1)
    };

    let mut tempo_us: u32 = 500_000;
    let mut first_tempo_seen = false;
    let mut max_ticks: u64 = 0;

    let mut pos = 14;
    while pos + 8 <= data.len() {
        if &data[pos..pos + 4] != b"MTrk" {
            break;
        }
        let len = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;
        let start = pos + 8;
        let end = (start + len).min(data.len());
        let mut p = start;
        let mut ticks: u64 = 0;
        let mut running_status: u8 = 0;
        while p < end {
            // Delta time (variable-length quantity).
            let (delta, np) = read_vlq(&data, p, end);
            p = np;
            ticks += delta as u64;
            if p >= end {
                break;
            }
            let mut status = data[p];
            if status < 0x80 {
                status = running_status; // running status
            } else {
                p += 1;
                running_status = status;
            }
            match status {
                0xF0 | 0xF7 => {
                    let (l, np) = read_vlq(&data, p, end);
                    p = np + l as usize;
                }
                0xFF => {
                    if p >= end {
                        break;
                    }
                    let meta = data[p];
                    p += 1;
                    let (l, np) = read_vlq(&data, p, end);
                    p = np;
                    if meta == 0x51 && l == 3 && p + 3 <= end && !first_tempo_seen {
                        tempo_us = ((data[p] as u32) << 16)
                            | ((data[p + 1] as u32) << 8)
                            | data[p + 2] as u32;
                        first_tempo_seen = true;
                    }
                    p += l as usize;
                }
                0x80..=0xEF => {
                    // Two data bytes, except program-change (0xC) / channel-pressure (0xD).
                    let hi = status & 0xF0;
                    p += if hi == 0xC0 || hi == 0xD0 { 1 } else { 2 };
                }
                _ => break, // malformed
            }
        }
        max_ticks = max_ticks.max(ticks);
        pos = end;
    }

    let duration_secs = max_ticks as f64 / ppq as f64 * (tempo_us as f64 / 1_000_000.0);
    Some(MidiInfo {
        ppq,
        tempo_us,
        duration_secs,
    })
}

/// Read a MIDI variable-length quantity at `p`; returns (value, next_pos).
#[cfg(test)]
fn read_vlq(data: &[u8], mut p: usize, end: usize) -> (u32, usize) {
    let mut value: u32 = 0;
    for _ in 0..4 {
        if p >= end {
            break;
        }
        let b = data[p];
        p += 1;
        value = (value << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            break;
        }
    }
    (value, p)
}

fn is_midi(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_lowercase().as_str(), "mid" | "midi"))
        .unwrap_or(false)
}

/// Locate a General-MIDI SoundFont: `$TANU_SOUNDFONT`, else the best `*.sf2` in
/// common dirs. Prefers full-GM banks (name contains "gm"/"general") and avoids
/// drum-kit fonts (e.g. `*_LV2.sf2`, names with "drum"/"perc") so melodic parts
/// aren't silenced.
fn find_soundfont() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TANU_SOUNDFONT") {
        let pb = PathBuf::from(shellexpand_tilde(&p));
        if pb.is_file() {
            return Some(pb);
        }
    }
    let mut dirs: Vec<PathBuf> = vec![
        PathBuf::from("/usr/share/sounds/sf2"),
        PathBuf::from("/usr/share/soundfonts"),
    ];
    if let Some(data) = dirs::data_dir() {
        dirs.push(data.join("tanu").join("soundfonts"));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("repo").join("free-soundfonts-sf2-2019-04"));
    }
    let mut best: Option<(i32, PathBuf)> = None;
    for dir in dirs {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if !p
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("sf2"))
                    .unwrap_or(false)
                {
                    continue;
                }
                let score = gm_score(&p);
                if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
                    best = Some((score, p));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Heuristic score for a SoundFont being a usable full-GM bank.
fn gm_score(path: &Path) -> i32 {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mut s = 0;
    if name.contains("gm") || name.contains("general") {
        s += 10;
    }
    if name.contains("fluidr3") || name.contains("timgm") || name.contains("generaluser") {
        s += 8;
    }
    // Drum-kit / effect fonts silence melodic channels — deprioritize hard.
    if name.contains("lv2") || name.contains("drum") || name.contains("perc") {
        s -= 20;
    }
    s
}

fn shellexpand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    p.to_string()
}

/// The rodio-based audio backend.
pub struct RodioBackend {
    #[allow(dead_code)]
    stream: OutputStream,
    handle: OutputStreamHandle,
    inner: Mutex<RodioInner>,
    viz: AudioViz,
    eq: EqState,
    soundfont: SharedSoundFont,
}

impl RodioBackend {
    pub fn new(viz: AudioViz, eq: EqState, soundfont: SharedSoundFont) -> anyhow::Result<Self> {
        let (stream, handle) = OutputStream::try_default()?;
        Ok(Self {
            stream,
            handle,
            viz,
            eq,
            soundfont,
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
        // MIDI: rodio/symphonia can't decode it. Render to PCM (WAV) via
        // fluidsynth, then play that through the normal pipeline — so seek, EQ,
        // and the oscilloscope all work exactly like any audio file.
        let (decoder, duration): (Decoder<BufReader<File>>, f64) = if is_midi(path) {
            let sf2 = self.soundfont.lock().unwrap().clone()
                .filter(|p| p.is_file())
                .or_else(find_soundfont)
                .ok_or_else(|| anyhow::anyhow!("No SoundFont (.sf2) found. Pick one from EDIT → SoundFont or set $TANU_SOUNDFONT."))?;
            let gain = self.inner.lock().unwrap().volume;
            let file = render_midi_to_pcm(&sf2, path, gain)?;
            let decoder = Decoder::new(BufReader::new(file))?;
            use rodio::Source;
            let dur = decoder
                .total_duration()
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            (decoder, dur)
        } else {
            // Duration from metadata (fast: reads headers, no full decode).
            let duration = lofty::read_from_path(path)
                .ok()
                .map(|f| {
                    use lofty::file::AudioFile;
                    f.properties().duration().as_secs_f64()
                })
                .unwrap_or(0.0);
            let file = File::open(path)?;
            (Decoder::new(BufReader::new(file))?, duration)
        };

        self.viz.on_play();
        // Chain: decode → EQ (modifies sound) → tap (viz shows post-EQ audio).
        let eqd = EqSource::new(decoder, self.eq.clone());
        let source = TappedSource::new(eqd, self.viz.clone());

        let sink = Sink::try_new(&self.handle)?;
        sink.set_volume(self.inner.lock().unwrap().volume);
        sink.append(source);

        let mut inner = self.inner.lock().unwrap();
        inner.sink = Some(sink); // dropping the old sink stops previous playback
        inner.current_duration = duration;
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
                self.viz.set_active(false);

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
                self.viz.set_active(true);
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
        self.viz.on_stop();
    }

    fn seek(&self, position_secs: f64) {
        let pos = position_secs.max(0.0);
        let mut inner = self.inner.lock().unwrap();
        let clamped = if inner.current_duration > 0.0 {
            pos.min(inner.current_duration)
        } else {
            pos
        };
        if let Some(ref sink) = inner.sink {
            // WAV/PCM (incl. rendered MIDI) seeks both directions; some streamed
            // codecs may not — best-effort.
            let _ = sink.try_seek(std::time::Duration::from_secs_f64(clamped));
        }
        inner.elapsed_before_pause = clamped;
        inner.started_at = if inner.paused {
            None
        } else {
            Some(Instant::now())
        };
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
        let cap = |t: f64| {
            if inner.current_duration > 0.0 {
                t.min(inner.current_duration)
            } else {
                t
            }
        };
        if inner.playing && !inner.paused {
            if let Some(start) = inner.started_at {
                return cap(inner.elapsed_before_pause + start.elapsed().as_secs_f64());
            }
        }
        cap(inner.elapsed_before_pause)
    }

    fn duration(&self) -> f64 {
        self.inner.lock().unwrap().current_duration
    }

    fn is_playing(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.playing && !inner.paused && inner.sink.as_ref().map(|s| !s.empty()).unwrap_or(false)
    }

    fn is_paused(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.paused && inner.sink.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_midi_duration() {
        // Format-0 SMF: division 96 PPQ, tempo 500000us (120bpm), 96 ticks long.
        let track: &[u8] = &[
            0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, // tempo = 500000
            0x00, 0x90, 0x3C, 0x40, // note on
            0x60, 0x80, 0x3C, 0x40, // +96 ticks, note off
            0x00, 0xFF, 0x2F, 0x00, // end of track
        ];
        let mut data = vec![
            b'M',
            b'T',
            b'h',
            b'd',
            0,
            0,
            0,
            6,
            0,
            0,
            0,
            1,
            0,
            96,
            b'M',
            b'T',
            b'r',
            b'k',
            0,
            0,
            0,
            track.len() as u8,
        ];
        data.extend_from_slice(track);
        let dir = std::env::temp_dir().join(format!("tanu-midi-{}.mid", std::process::id()));
        std::fs::write(&dir, &data).unwrap();
        let info = parse_midi(&dir).unwrap();
        assert_eq!(info.ppq, 96);
        assert_eq!(info.tempo_us, 500_000);
        assert!(
            (info.duration_secs - 0.5).abs() < 1e-6,
            "got {}",
            info.duration_secs
        );
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn test_gm_score_prefers_full_gm_over_drumkit() {
        let gm = gm_score(Path::new("/x/FluidR3_GM.sf2"));
        let drum = gm_score(Path::new("/x/Black_Pearl_4_LV2.sf2"));
        assert!(gm > drum, "GM font ({gm}) must outrank drum kit ({drum})");
        assert!(
            gm_score(Path::new("/x/default-GM.sf2"))
                > gm_score(Path::new("/x/Red_Zeppelin_4_LV2.sf2"))
        );
    }
}
