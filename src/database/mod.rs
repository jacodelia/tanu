//! SQLite database layer.
//!
//! Wraps rusqlite for library persistence. All queries run via
//! `spawn_blocking` to avoid blocking the async runtime.

use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use parking_lot::Mutex;

/// Database handle, shared across components.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Opens (or creates) the database at `path`.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.initialize_schema()?;
        Ok(db)
    }

    /// Creates tables and indexes if they don't exist.
    fn initialize_schema(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS artists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL COLLATE NOCASE,
                sort_name TEXT COLLATE NOCASE,
                musicbrainz_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_artists_name ON artists(name);

            CREATE TABLE IF NOT EXISTS albums (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL COLLATE NOCASE,
                artist_id TEXT REFERENCES artists(id),
                year INTEGER,
                genre TEXT,
                cover_path TEXT,
                track_count INTEGER DEFAULT 0,
                duration_secs REAL DEFAULT 0.0,
                musicbrainz_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_albums_title ON albums(title);
            CREATE INDEX IF NOT EXISTS idx_albums_artist ON albums(artist_id);

            CREATE TABLE IF NOT EXISTS tracks (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL COLLATE NOCASE,
                artist_id TEXT REFERENCES artists(id),
                album_id TEXT REFERENCES albums(id),
                track_number INTEGER,
                disc_number INTEGER DEFAULT 1,
                duration_secs REAL DEFAULT 0.0,
                bitrate_kbps INTEGER,
                sample_rate_hz INTEGER,
                channels INTEGER,
                file_size_bytes INTEGER,
                file_modified_secs INTEGER,
                genre TEXT,
                year INTEGER,
                format TEXT,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist_id);
            CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album_id);
            CREATE INDEX IF NOT EXISTS idx_tracks_path ON tracks(path);
            CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre);

            CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
                title, artist_name, album_title, genre,
                content='tracks',
                content_rowid='rowid'
            );

            -- Triggers to keep FTS5 index in sync
            CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
                INSERT INTO tracks_fts(rowid, title, artist_name, album_title, genre)
                SELECT new.rowid, new.title, a.name, al.title, new.genre
                FROM artists a, albums al
                WHERE a.id = new.artist_id AND al.id = new.album_id;
            END;

            CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
                INSERT INTO tracks_fts(tracks_fts, rowid, title, artist_name, album_title, genre)
                VALUES ('delete', old.rowid, old.title, '', '', '');
            END;

            CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE ON tracks BEGIN
                INSERT INTO tracks_fts(tracks_fts, rowid, title, artist_name, album_title, genre)
                VALUES ('delete', old.rowid, old.title, '', '', '');
                INSERT INTO tracks_fts(rowid, title, artist_name, album_title, genre)
                SELECT new.rowid, new.title, a.name, al.title, new.genre
                FROM artists a, albums al
                WHERE a.id = new.artist_id AND al.id = new.album_id;
            END;

            CREATE TABLE IF NOT EXISTS playlists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now')),
                track_count INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS playlist_tracks (
                playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                added_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (playlist_id, track_id)
            );
            CREATE INDEX IF NOT EXISTS idx_playlist_tracks_playlist ON playlist_tracks(playlist_id, position);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "
        )?;
        Ok(())
    }

    /// Returns a clone of the inner connection for direct use.
    /// Prefer using typed query methods instead.
    pub fn with_connection<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self.conn.lock();
        f(&conn)
    }
}

pub mod migrations;
pub mod queries;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_open_and_schema() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).unwrap();

        // Verify tables exist
        db.with_connection(|conn| {
            let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
            let tables: Vec<String> = stmt.query_map([], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            assert!(tables.contains(&"artists".to_string()));
            assert!(tables.contains(&"albums".to_string()));
            assert!(tables.contains(&"tracks".to_string()));
            assert!(tables.contains(&"playlists".to_string()));
            assert!(tables.contains(&"playlist_tracks".to_string()));
            Ok(())
        }).unwrap();
    }

    #[test]
    fn test_database_insert_artist() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).unwrap();

        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO artists (id, name) VALUES (?1, ?2)",
                &["artist-1", "Test Artist"],
            )?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM artists WHERE name = 'Test Artist'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(count, 1);
            Ok(())
        }).unwrap();
    }
}
