//! In-memory metadata cache using LRU eviction.
//!
//! Reduces repeated disk I/O for tracks that are frequently accessed
//! (e.g., currently playing, visible in the library view).

use std::num::NonZeroUsize;
use std::path::Path;

use lru::LruCache;

use super::indexer::{index_file, IndexedTrack};

/// Default cache capacity: 5000 tracks.
const DEFAULT_CAPACITY: usize = 5000;

/// A thread-safe cache of indexed track metadata.
pub struct MetadataCache {
    cache: LruCache<String, IndexedTrack>,
}

impl MetadataCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(capacity).unwrap_or(
                NonZeroUsize::new(DEFAULT_CAPACITY).unwrap(),
            )),
        }
    }

    /// Get a track from the cache, or index it from disk and cache it.
    pub fn get_or_index(&mut self, path: &Path) -> Option<IndexedTrack> {
        let key = path.to_string_lossy().to_string();

        if let Some(cached) = self.cache.get(&key) {
            return Some(cached.clone());
        }

        match index_file(path) {
            Ok(Some(track)) => {
                self.cache.put(key.clone(), track.clone());
                Some(track)
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to index file");
                None
            }
        }
    }

    /// Invalidate a cached entry.
    pub fn invalidate(&mut self, path: &Path) {
        let key = path.to_string_lossy().to_string();
        self.cache.pop(&key);
    }

    /// Check if an entry is cached and still valid (mtime matches).
    pub fn is_fresh(&self, path: &Path, mtime_secs: i64) -> bool {
        let key = path.to_string_lossy().to_string();
        self.cache
            .peek(&key)
            .map(|t| t.file_modified_secs == mtime_secs)
            .unwrap_or(false)
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.cache.len()
    }
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}
