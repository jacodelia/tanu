//! Audio file indexer using lofty for metadata extraction.
//!
//! Extracts: title, artist, album, track number, disc number,
//! year, genre, duration, bitrate, sample rate, channels,
//! file size, and format from audio files.

use std::path::Path;
use std::time::UNIX_EPOCH;

use lofty::file::AudioFile;
use lofty::file::TaggedFileExt;
use lofty::tag::Accessor;

/// Metadata extracted from an audio file, ready for database insertion.
#[derive(Debug, Clone)]
pub struct IndexedTrack {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub duration_secs: f64,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u32>,
    pub file_size_bytes: u64,
    pub file_modified_secs: i64,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub format: String,
}

impl IndexedTrack {
    fn fallback_title(path: &Path) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string()
    }
}

/// Extract metadata from an audio file using lofty.
pub fn index_file(path: &Path) -> anyhow::Result<Option<IndexedTrack>> {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "Cannot stat file");
            return Ok(None);
        }
    };

    let file_size_bytes = metadata.len();
    let file_modified_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown")
        .to_lowercase();

    let tagged_file = match lofty::read_from_path(path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "Cannot read tags");
            return Ok(Some(IndexedTrack {
                path: path.to_string_lossy().to_string(),
                title: IndexedTrack::fallback_title(path),
                artist: "Unknown Artist".to_string(),
                album: "Unknown Album".to_string(),
                album_artist: None,
                track_number: None,
                disc_number: None,
                duration_secs: 0.0,
                bitrate_kbps: None,
                sample_rate_hz: None,
                channels: None,
                file_size_bytes,
                file_modified_secs,
                genre: None,
                year: None,
                format,
            }));
        }
    };

    let properties = tagged_file.properties();
    let duration_secs = properties.duration().as_secs_f64();
    let sample_rate_hz = properties.sample_rate();
    let channels = properties.channels().map(|c| c as u32);
    let bitrate_kbps = properties.audio_bitrate().map(|b| b / 1000);

    let tag = tagged_file.primary_tag();

    let title = tag
        .and_then(|t| t.title().map(|s| s.to_string()))
        .unwrap_or_else(|| IndexedTrack::fallback_title(path));

    let artist = tag
        .and_then(|t| t.artist().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown Artist".to_string());

    let album = tag
        .and_then(|t| t.album().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown Album".to_string());

    let album_artist = tag.and_then(|t| {
        t.get_string(&lofty::tag::ItemKey::AlbumArtist)
            .map(|s| s.to_string())
    });

    let track_number = tag.and_then(|t| t.track());

    let disc_number = tag.and_then(|t| t.disk());

    let genre = tag.and_then(|t| t.genre().map(|s| s.to_string()));

    let year = tag.and_then(|t| t.year().map(|y| y as i32));

    Ok(Some(IndexedTrack {
        path: path.to_string_lossy().to_string(),
        title,
        artist,
        album,
        album_artist,
        track_number,
        disc_number,
        duration_secs,
        bitrate_kbps,
        sample_rate_hz,
        channels,
        file_size_bytes,
        file_modified_secs,
        genre,
        year,
        format,
    }))
}
