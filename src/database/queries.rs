//! Database query helpers for the library scanner.
//!
//! Provides idempotent insert/upsert for artists, albums, and tracks
//! with foreign-key resolution.

use rusqlite::{params, Connection};
use uuid::Uuid;
use std::path::Path;

use crate::library::indexer::IndexedTrack;

const TANU_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1,
    0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
]);

/// Generate a deterministic UUID v5 from a seed string.
fn hash_id(seed: &str) -> String {
    Uuid::new_v5(&TANU_NAMESPACE, seed.as_bytes()).to_string()
}

/// Insert or get an artist. Returns the artist ID.
pub fn upsert_artist(conn: &Connection, name: &str) -> anyhow::Result<String> {
    let id = hash_id(&format!("artist:{}", name.to_lowercase()));

    conn.execute(
        "INSERT OR IGNORE INTO artists (id, name) VALUES (?1, ?2)",
        params![id, name],
    )?;

    Ok(id)
}

/// Insert or get an album. Returns the album ID.
pub fn upsert_album(
    conn: &Connection,
    title: &str,
    artist_id: &str,
    year: Option<i32>,
    genre: Option<&str>,
) -> anyhow::Result<String> {
    let id = hash_id(&format!("album:{}:{}", artist_id, title.to_lowercase()));

    conn.execute(
        "INSERT OR IGNORE INTO albums (id, title, artist_id, year, genre) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, title, artist_id, year, genre],
    )?;

    Ok(id)
}

/// Insert or replace a track. Returns the track ID.
pub fn upsert_track(conn: &Connection, track: &IndexedTrack) -> anyhow::Result<String> {
    let artist_id = upsert_artist(conn, &track.artist)?;
    let album_id = upsert_album(
        conn,
        &track.album,
        &artist_id,
        track.year,
        track.genre.as_deref(),
    )?;

    // Check if track already exists by path
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM tracks WHERE path = ?1",
            params![track.path],
            |row| row.get(0),
        )
        .ok();

    let id = if let Some(ref existing_id) = existing {
        // Update existing track
        conn.execute(
            "UPDATE tracks SET
                title = ?2, artist_id = ?3, album_id = ?4,
                track_number = ?5, disc_number = ?6,
                duration_secs = ?7, bitrate_kbps = ?8,
                sample_rate_hz = ?9, channels = ?10,
                file_size_bytes = ?11, file_modified_secs = ?12,
                genre = ?13, year = ?14, format = ?15
             WHERE id = ?1",
            params![
                existing_id,
                track.title,
                artist_id,
                album_id,
                track.track_number,
                track.disc_number,
                track.duration_secs,
                track.bitrate_kbps,
                track.sample_rate_hz,
                track.channels,
                track.file_size_bytes as i64,
                track.file_modified_secs,
                track.genre,
                track.year,
                track.format,
            ],
        )?;
        existing_id.clone()
    } else {
        let new_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO tracks (
                id, path, title, artist_id, album_id,
                track_number, disc_number, duration_secs,
                bitrate_kbps, sample_rate_hz, channels,
                file_size_bytes, file_modified_secs, genre, year, format
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, ?16
            )",
            params![
                new_id,
                track.path,
                track.title,
                artist_id,
                album_id,
                track.track_number,
                track.disc_number,
                track.duration_secs,
                track.bitrate_kbps,
                track.sample_rate_hz,
                track.channels,
                track.file_size_bytes as i64,
                track.file_modified_secs,
                track.genre,
                track.year,
                track.format,
            ],
        )?;
        new_id
    };

    Ok(id)
}

/// Delete tracks whose path starts with a given prefix (for directory removal).
pub fn delete_tracks_in_dir(conn: &Connection, dir: &Path) -> anyhow::Result<usize> {
    let prefix = format!("{}/", dir.to_string_lossy());
    let count = conn.execute(
        "DELETE FROM tracks WHERE path LIKE ?1 || '%'",
        params![prefix],
    )?;
    Ok(count)
}

/// Get all track paths currently in the database.
pub fn all_track_paths(conn: &Connection) -> anyhow::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare("SELECT path, file_modified_secs FROM tracks")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

/// Remove tracks not in the given path set (stale entries).
pub fn remove_stale_tracks(conn: &Connection, current_paths: &[String]) -> anyhow::Result<usize> {
    if current_paths.is_empty() {
        return Ok(0);
    }

    let placeholders: Vec<String> = current_paths.iter().enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect();

    let sql = format!(
        "DELETE FROM tracks WHERE path NOT IN ({})",
        placeholders.join(", ")
    );

    let params: Vec<&dyn rusqlite::types::ToSql> = current_paths
        .iter()
        .map(|p| p as &dyn rusqlite::types::ToSql)
        .collect();

    let count = conn.execute(&sql, params.as_slice())?;
    Ok(count)
}

/// Search tracks using FTS5.
pub fn search_fts(conn: &Connection, query: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.id FROM tracks t
         JOIN tracks_fts fts ON t.rowid = fts.rowid
         WHERE tracks_fts MATCH ?1
         ORDER BY rank
         LIMIT 100",
    )?;
    let rows = stmt
        .query_map(params![query], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}
