//! Seekable symphonia-backed audio source.
//!
//! rodio 0.19's own decoders can't seek FLAC: the native (claxon) decoder
//! returns `NotSupported`, and rodio's symphonia path wraps the file in a
//! `MediaSource` whose `byte_len()` is `None`, which makes symphonia report the
//! stream unseekable. Here we hand symphonia the `std::fs::File` directly — it
//! implements `MediaSource` with a real `byte_len`, so in-place seeking works
//! (via the FLAC seektable or byte-range bisection). Decode/iterator logic is
//! ported from rodio's proven `decoder::symphonia`.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use rodio::source::SeekError;
use rodio::Source;
use symphonia::core::audio::{AudioBufferRef, SampleBuffer, SignalSpec};
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, SeekedTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{self, Time};

// A decode error in more than 3 consecutive packets is fatal.
const MAX_DECODE_RETRIES: usize = 3;

pub struct FlacSource {
    decoder: Box<dyn Decoder>,
    current_frame_offset: usize,
    format: Box<dyn FormatReader>,
    total_duration: Option<Time>,
    buffer: SampleBuffer<i16>,
    spec: SignalSpec,
}

impl FlacSource {
    /// Open an audio file with a fully seekable symphonia stream.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        // File implements MediaSource with a real byte_len → seekable stream.
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let ext = path.extension().and_then(|e| e.to_str());
        Self::init(mss, ext)?.ok_or_else(|| anyhow::anyhow!("no decodable audio track"))
    }

    fn init(mss: MediaSourceStream, extension: Option<&str>) -> anyhow::Result<Option<Self>> {
        let mut hint = Hint::new();
        if let Some(ext) = extension {
            hint.with_extension(ext);
        }
        let format_opts = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };
        let metadata_opts: MetadataOptions = Default::default();
        let probed = symphonia::default::get_probe().format(
            &hint,
            mss,
            &format_opts,
            &metadata_opts,
        )?;
        let mut format = probed.format;

        let track = match format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        {
            Some(t) => t,
            None => return Ok(None),
        };
        let track_id = track.id;
        let codec_params = track.codec_params.clone();
        let total_duration = codec_params
            .time_base
            .zip(codec_params.n_frames)
            .map(|(base, frames)| base.calc_time(frames));

        let mut decoder =
            symphonia::default::get_codecs().make(&codec_params, &DecoderOptions::default())?;

        let mut decode_errors: usize = 0;
        let decoded = loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(_)) => break decoder.last_decoded(),
                Err(e) => return Err(e.into()),
            };
            if packet.track_id() != track_id {
                continue;
            }
            match decoder.decode(&packet) {
                Ok(d) => break d,
                Err(Error::DecodeError(_)) if decode_errors < MAX_DECODE_RETRIES => {
                    decode_errors += 1;
                }
                Err(e) => return Err(e.into()),
            }
        };
        let spec = decoded.spec().to_owned();
        let buffer = Self::get_buffer(decoded, &spec);
        Ok(Some(Self {
            decoder,
            current_frame_offset: 0,
            format,
            total_duration,
            buffer,
            spec,
        }))
    }

    #[inline]
    fn get_buffer(decoded: AudioBufferRef, spec: &SignalSpec) -> SampleBuffer<i16> {
        let duration = units::Duration::from(decoded.capacity() as u64);
        let mut buffer = SampleBuffer::<i16>::new(duration, *spec);
        buffer.copy_interleaved_ref(decoded);
        buffer
    }

    /// After `format.seek`, decode forward to the exact requested timestamp.
    fn refine_position(&mut self, seek_res: SeekedTo) -> Result<(), SeekError> {
        let mut samples_to_pass = seek_res.required_ts.saturating_sub(seek_res.actual_ts);
        let packet = loop {
            let candidate = self
                .format
                .next_packet()
                .map_err(|_| SeekError::NotSupported { underlying_source: "flac" })?;
            if candidate.dur() > samples_to_pass {
                break candidate;
            }
            samples_to_pass -= candidate.dur();
        };
        let mut decoded = self.decoder.decode(&packet);
        for _ in 0..MAX_DECODE_RETRIES {
            if decoded.is_err() {
                let packet = self
                    .format
                    .next_packet()
                    .map_err(|_| SeekError::NotSupported { underlying_source: "flac" })?;
                decoded = self.decoder.decode(&packet);
            }
        }
        let decoded = decoded.map_err(|_| SeekError::NotSupported { underlying_source: "flac" })?;
        decoded.spec().clone_into(&mut self.spec);
        self.buffer = Self::get_buffer(decoded, &self.spec);
        self.current_frame_offset = samples_to_pass as usize * self.channels() as usize;
        Ok(())
    }
}

impl Iterator for FlacSource {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if self.current_frame_offset >= self.buffer.len() {
            let packet = self.format.next_packet().ok()?;
            let mut decoded = self.decoder.decode(&packet);
            for _ in 0..MAX_DECODE_RETRIES {
                if decoded.is_err() {
                    let packet = self.format.next_packet().ok()?;
                    decoded = self.decoder.decode(&packet);
                }
            }
            let decoded = decoded.ok()?;
            decoded.spec().clone_into(&mut self.spec);
            self.buffer = Self::get_buffer(decoded, &self.spec);
            self.current_frame_offset = 0;
        }
        let sample = *self.buffer.samples().get(self.current_frame_offset)?;
        self.current_frame_offset += 1;
        Some(sample)
    }
}

impl Source for FlacSource {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.buffer.samples().len() - self.current_frame_offset)
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.spec.channels.count() as u16
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.spec.rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
            .map(|Time { seconds, frac }| Duration::new(seconds, (frac * 1e9) as u32))
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        // Ensure the resumed sample lands on the right channel.
        let to_skip = self.current_frame_offset % self.channels() as usize;
        let seek_res = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: pos.as_secs_f64().into(),
                    track_id: None,
                },
            )
            .map_err(|_| SeekError::NotSupported { underlying_source: "flac" })?;
        self.decoder.reset();
        self.refine_position(seek_res)?;
        self.current_frame_offset += to_skip;
        Ok(())
    }
}
