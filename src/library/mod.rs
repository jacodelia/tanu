//! Music library indexer and scanner.
//!
//! Walks configured directories, extracts metadata via lofty,
//! populates the database, supports incremental scanning (mtime),
//! and filesystem watching via `notify`.

use std::path::PathBuf;
use std::sync::mpsc;

use notify::{EventKind, RecursiveMode, Watcher};
use walkdir::WalkDir;

use crate::database::queries;
use crate::database::Database;
use crate::events::bus::EventSender;
use crate::events::Event;

pub mod cache;
pub mod indexer;

use cache::MetadataCache;
use indexer::index_file;

/// Supported audio file extensions.
const DEFAULT_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wma"];

/// The library handles the music collection: scanning, indexing,
/// and responding to filesystem changes.
pub struct Library {
    db: Database,
    music_dirs: Vec<PathBuf>,
    extensions: Vec<String>,
    is_scanning: bool,
    cache: MetadataCache,
}

/// The result of a library scan operation.
#[derive(Debug, Default)]
pub struct ScanResult {
    pub files_found: usize,
    pub tracks_added: usize,
    pub tracks_updated: usize,
    pub tracks_removed: usize,
    pub duration_secs: f64,
}

impl Library {
    pub fn new(db: Database, music_dirs: Vec<PathBuf>) -> Self {
        Self {
            db,
            music_dirs,
            extensions: DEFAULT_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
            is_scanning: false,
            cache: MetadataCache::default(),
        }
    }

    /// Returns true if a scan is in progress.
    pub fn is_scanning(&self) -> bool {
        self.is_scanning
    }

    /// Performs a full scan of all configured music directories.
    /// Extracts metadata and inserts into the database.
    /// Call via `spawn_blocking` in production.
    pub fn scan(&mut self, event_tx: &EventSender) -> anyhow::Result<ScanResult> {
        self.is_scanning = true;
        let mut result = ScanResult::default();
        let mut found_paths: Vec<String> = Vec::new();
        let start = std::time::Instant::now();

        for dir in &self.music_dirs.clone() {
            if !dir.exists() {
                tracing::warn!(dir = %dir.display(), "Music directory does not exist");
                continue;
            }

            let entries: Vec<PathBuf> = WalkDir::new(dir)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| {
                            self.extensions
                                .contains(&ext.to_string_lossy().to_lowercase())
                        })
                        .unwrap_or(false)
                })
                .map(|e| e.path().to_path_buf())
                .collect();

            for (i, path) in entries.iter().enumerate() {
                result.files_found += 1;
                found_paths.push(path.to_string_lossy().to_string());

                let mtime = std::fs::metadata(path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                // Check if this file is already indexed and unchanged
                let needs_index = !self.cache.is_fresh(path, mtime);

                if needs_index {
                    self.db.with_connection(|conn| match index_file(path) {
                        Ok(Some(track)) => {
                            let existing_id: Option<String> = conn
                                .query_row(
                                    "SELECT id FROM tracks WHERE path = ?1",
                                    rusqlite::params![track.path],
                                    |row| row.get(0),
                                )
                                .ok();

                            if existing_id.is_some() {
                                result.tracks_updated += 1;
                            } else {
                                result.tracks_added += 1;
                            }

                            queries::upsert_track(conn, &track)?;
                            self.cache.invalidate(path);
                            Ok(())
                        }
                        Ok(None) => Ok(()),
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Failed to index file"
                            );
                            Ok(())
                        }
                    })?;
                }

                // Emit progress every 50 files
                if i % 50 == 0 {
                    let _ = event_tx.send(Event::LibraryScanProgress {
                        tracks_found: result.files_found,
                        tracks_processed: i + 1,
                    });
                }
            }
        }

        // Remove stale tracks that no longer exist on disk
        self.db.with_connection(|conn| {
            result.tracks_removed = queries::remove_stale_tracks(conn, &found_paths)?;
            Ok(())
        })?;

        result.duration_secs = start.elapsed().as_secs_f64();
        self.is_scanning = false;

        let _ = event_tx.send(Event::LibraryScanComplete {
            total_tracks: self.track_count()?,
            duration_secs: result.duration_secs,
        });

        Ok(result)
    }

    /// Start a filesystem watcher on all music directories.
    /// Runs on a dedicated OS thread, sending events via the event bus.
    pub fn start_watcher(music_dirs: Vec<PathBuf>, event_tx: EventSender) -> anyhow::Result<()> {
        let (watcher_tx, watcher_rx) = mpsc::channel();

        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = watcher_tx.send(event);
                }
            })?;

        for dir in &music_dirs {
            if dir.exists() {
                watcher.watch(dir, RecursiveMode::Recursive)?;
            }
        }

        // Spawn a thread that processes filesystem events
        std::thread::spawn(move || {
            // Keep watcher alive
            let _watcher = watcher;

            while let Ok(event) = watcher_rx.recv() {
                let mut added = Vec::new();
                let mut removed = Vec::new();
                let mut modified = Vec::new();

                for path in &event.paths {
                    let path_str = path.to_string_lossy().to_string();
                    match event.kind {
                        EventKind::Create(_) => added.push(path_str),
                        EventKind::Remove(_) => removed.push(path_str),
                        EventKind::Modify(_) => modified.push(path_str),
                        _ => {}
                    }
                }

                if !added.is_empty() || !removed.is_empty() || !modified.is_empty() {
                    let _ = event_tx.send(Event::LibraryFilesChanged {
                        added,
                        removed,
                        modified,
                    });
                }
            }
        });

        Ok(())
    }

    /// Returns the total number of tracks in the library.
    pub fn track_count(&self) -> anyhow::Result<usize> {
        self.db.with_connection(|conn| {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))?;
            Ok(count as usize)
        })
    }

    /// Get paths of all tracks in the library.
    pub fn all_track_paths(&self) -> anyhow::Result<Vec<PathBuf>> {
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT path FROM tracks ORDER BY path")?;
            let paths: Vec<PathBuf> = stmt
                .query_map([], |row| {
                    let p: String = row.get(0)?;
                    Ok(PathBuf::from(p))
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok(paths)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    /// Generate a minimal WAV file for testing.
    fn write_test_wav(path: &Path, sample_rate: u32, duration_secs: f64) {
        let num_samples = (duration_secs * sample_rate as f64) as usize;
        let data_size = (num_samples * 2) as u32;
        let riff_size = 36 + data_size;

        let mut file = std::fs::File::create(path).unwrap();
        use std::io::Write;

        file.write_all(b"RIFF").unwrap();
        file.write_all(&riff_size.to_le_bytes()).unwrap();
        file.write_all(b"WAVE").unwrap();

        file.write_all(b"fmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        file.write_all(&1u16.to_le_bytes()).unwrap(); // mono
        file.write_all(&sample_rate.to_le_bytes()).unwrap();
        file.write_all(&(sample_rate * 2).to_le_bytes()).unwrap();
        file.write_all(&2u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();

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
    fn test_library_creation() {
        let dir = TempDir::new().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let lib = Library::new(db, vec![dir.path().to_path_buf()]);
        assert!(!lib.is_scanning());
    }

    #[test]
    fn test_library_scan_empty_dir() {
        let dir = TempDir::new().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let mut lib = Library::new(db, vec![dir.path().to_path_buf()]);

        let (tx, _rx) = crate::events::bus::event_channel();
        let result = lib.scan(&tx).unwrap();
        assert_eq!(result.files_found, 0);
    }

    #[test]
    fn test_library_scan_with_wav() {
        let dir = TempDir::new().unwrap();

        // Create a test WAV
        let wav_path = dir.path().join("test.wav");
        write_test_wav(&wav_path, 44100, 0.1);

        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let mut lib = Library::new(db, vec![dir.path().to_path_buf()]);

        let (tx, _rx) = crate::events::bus::event_channel();
        let result = lib.scan(&tx).unwrap();

        assert_eq!(result.files_found, 1);
        assert_eq!(result.tracks_added, 1);
        assert_eq!(lib.track_count().unwrap(), 1);
    }

    #[test]
    fn test_library_scan_handles_non_audio() {
        let dir = TempDir::new().unwrap();

        // Create a non-audio file
        std::fs::write(dir.path().join("notes.txt"), b"hello").unwrap();

        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let mut lib = Library::new(db, vec![dir.path().to_path_buf()]);

        let (tx, _rx) = crate::events::bus::event_channel();
        let result = lib.scan(&tx).unwrap();

        assert_eq!(result.files_found, 0);
        assert_eq!(lib.track_count().unwrap(), 0);
    }

    #[test]
    fn test_track_count_empty() {
        let dir = TempDir::new().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let lib = Library::new(db, vec![dir.path().to_path_buf()]);
        assert_eq!(lib.track_count().unwrap(), 0);
    }
}
