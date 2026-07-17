//! Audio decoder using symphonia.
//!
//! Opens audio files, detects format via `Probe`,
//! decodes all packets into interleaved f32 samples,
//! and returns a `DecodedAudio` struct ready for playback.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decoded audio ready for playback.
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
}

/// Decode an audio file at `path` into PCM f32 samples.
pub fn decode_file(path: &Path) -> anyhow::Result<DecodedAudio> {
    let src = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let meta_opts = MetadataOptions::default();
    let fmt_opts = FormatOptions::default();

    let probed = symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts)?;

    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| anyhow::anyhow!("no default audio track found"))?;

    let codec_params = track.codec_params.clone();
    let dec_opts = DecoderOptions::default();
    let mut decoder = symphonia::default::get_codecs().make(&codec_params, &dec_opts)?;

    let track_id = track.id;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut raw_samples: Vec<f32> = Vec::new();
    let mut spec = None;

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };

        if spec.is_none() {
            spec = Some(*decoded.spec());
            let s = spec.as_ref().unwrap();
            sample_buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, *s));
        }

        if let Some(ref mut buf) = sample_buf {
            buf.copy_interleaved_ref(decoded);
            raw_samples.extend_from_slice(buf.samples());
        }
    }

    let spec = spec.ok_or_else(|| anyhow::anyhow!("no audio data decoded"))?;
    let sample_count = raw_samples.len();
    let channels = spec.channels.count() as u16;
    let sample_rate = spec.rate;

    let duration_secs = if sample_rate > 0 && channels > 0 {
        sample_count as f64 / (sample_rate as f64 * channels as f64)
    } else {
        0.0
    };

    Ok(DecodedAudio {
        samples: raw_samples,
        sample_rate,
        channels,
        duration_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a minimal 16-bit PCM WAV file with a sine tone.
    fn generate_test_wav(path: &Path, duration_secs: f64, sample_rate: u32) {
        let num_samples = (duration_secs * sample_rate as f64) as usize;
        let data_size = (num_samples * 2) as u32; // 16-bit mono
        let riff_size = 36 + data_size;

        let mut file = std::fs::File::create(path).unwrap();
        use std::io::Write;

        // RIFF header
        file.write_all(b"RIFF").unwrap();
        file.write_all(&riff_size.to_le_bytes()).unwrap();
        file.write_all(b"WAVE").unwrap();

        // fmt chunk
        file.write_all(b"fmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap(); // chunk size
        file.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        file.write_all(&1u16.to_le_bytes()).unwrap(); // mono
        file.write_all(&sample_rate.to_le_bytes()).unwrap();
        file.write_all(&(sample_rate * 2).to_le_bytes()).unwrap(); // byte rate
        file.write_all(&2u16.to_le_bytes()).unwrap(); // block align
        file.write_all(&16u16.to_le_bytes()).unwrap(); // bits per sample

        // data chunk
        file.write_all(b"data").unwrap();
        file.write_all(&data_size.to_le_bytes()).unwrap();

        for i in 0..num_samples {
            let t = i as f64 / sample_rate as f64;
            let sample = (t * 440.0 * 2.0 * std::f64::consts::PI).sin();
            let amplitude = (sample * 0.3 * i16::MAX as f64) as i16;
            file.write_all(&amplitude.to_le_bytes()).unwrap();
        }
    }

    #[test]
    fn test_decode_nonexistent_file() {
        let result = decode_file(Path::new("/nonexistent/audio.mp3"));
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_empty_file_fails_gracefully() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("empty.mp3");
        std::fs::write(&path, b"").unwrap();
        let result = decode_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_wav_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.wav");
        generate_test_wav(&path, 0.1, 44100);

        let decoded = decode_file(&path).expect("failed to decode test WAV");

        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.sample_rate, 44100);
        assert!(decoded.samples.len() > 100);
        assert!(decoded.duration_secs > 0.05);
        assert!(decoded.duration_secs < 0.15);
    }

    #[test]
    fn test_decode_stereo_wav() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("stereo.wav");
        let sample_rate = 44100u32;
        let num_samples = (0.05 * sample_rate as f64) as usize;
        let data_size = (num_samples * 4) as u32; // 16-bit stereo
        let riff_size = 36 + data_size;

        let mut file = std::fs::File::create(&path).unwrap();
        use std::io::Write;

        file.write_all(b"RIFF").unwrap();
        file.write_all(&riff_size.to_le_bytes()).unwrap();
        file.write_all(b"WAVE").unwrap();

        file.write_all(b"fmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        file.write_all(&2u16.to_le_bytes()).unwrap(); // stereo
        file.write_all(&sample_rate.to_le_bytes()).unwrap();
        file.write_all(&(sample_rate * 4).to_le_bytes()).unwrap(); // byte rate
        file.write_all(&4u16.to_le_bytes()).unwrap(); // block align
        file.write_all(&16u16.to_le_bytes()).unwrap(); // bits per sample

        file.write_all(b"data").unwrap();
        file.write_all(&data_size.to_le_bytes()).unwrap();

        for i in 0..num_samples {
            let t = i as f64 / sample_rate as f64;
            let sample = (t * 220.0 * 2.0 * std::f64::consts::PI).sin();
            let amplitude = (sample * 0.3 * i16::MAX as f64) as i16;
            file.write_all(&amplitude.to_le_bytes()).unwrap();
            file.write_all(&amplitude.to_le_bytes()).unwrap();
        }

        let decoded = decode_file(&path).expect("failed to decode stereo WAV");

        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.sample_rate, 44100);
        assert!(decoded.samples.len() > 100);
    }
}
