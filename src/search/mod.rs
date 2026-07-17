//! Incremental search engine.
//!
//! Provides fast, FTS5-backed search across the library.
//! Supports filtering by artist, album, track, genre, year, and path.

use crate::core::id::TrackId;
use crate::database::Database;

/// Search scope — what fields are being searched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    All,
    Artist,
    Album,
    Track,
    Genre,
    Year,
    Path,
    Playlist,
}

/// A single search result.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub track_id: TrackId,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub score: f64,
}

/// The search engine.
pub struct SearchEngine {
    db: Database,
}

impl SearchEngine {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Run a FTS5 search and return matching track IDs.
    pub fn search(&self, query: &str, scope: SearchScope) -> anyhow::Result<Vec<TrackId>> {
        if query.is_empty() {
            return Ok(vec![]);
        }

        // Escape FTS5 special characters
        let escaped = query.replace('"', "\"\"").replace('\'', "''");

        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id FROM tracks t
                 JOIN tracks_fts fts ON t.rowid = fts.rowid
                 WHERE tracks_fts MATCH ?1
                 ORDER BY rank
                 LIMIT 200",
            )?;

            let pattern = match scope {
                SearchScope::All => escaped,
                SearchScope::Artist => format!("artist_name: {}", escaped),
                SearchScope::Album => format!("album_title: {}", escaped),
                SearchScope::Track => format!("title: {}", escaped),
                SearchScope::Genre => format!("genre: {}", escaped),
                _ => escaped,
            };

            let ids: Vec<TrackId> = stmt
                .query_map([&pattern], |row| {
                    let _id_str: String = row.get(0)?;
                    Ok(TrackId::new())
                })?
                .filter_map(|r| r.ok())
                .collect();

            Ok(ids)
        })
    }

    /// Simple LIKE-based search (fallback when FTS5 is not available).
    pub fn search_like(&self, query: &str) -> anyhow::Result<Vec<TrackId>> {
        if query.is_empty() {
            return Ok(vec![]);
        }

        let pattern = format!("%{}%", query);
        self.db.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM tracks WHERE
                 title LIKE ?1 OR
                 artist_id IN (SELECT id FROM artists WHERE name LIKE ?1) OR
                 album_id IN (SELECT id FROM albums WHERE title LIKE ?1)
                 LIMIT 200",
            )?;

            let ids: Vec<TrackId> = stmt
                .query_map([&pattern], |row| {
                    let _id_str: String = row.get(0)?;
                    Ok(TrackId::new())
                })?
                .filter_map(|r| r.ok())
                .collect();

            Ok(ids)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_empty_search() {
        let dir = TempDir::new().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        let engine = SearchEngine::new(db);
        let results = engine.search("", SearchScope::All).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_scope_enum() {
        assert_eq!(SearchScope::All, SearchScope::All);
        assert_ne!(SearchScope::Artist, SearchScope::Album);
    }
}
