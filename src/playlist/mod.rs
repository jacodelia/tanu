//! Playlist management.
//!
//! Playlists are ordered collections of tracks. They are stored
//! in SQLite and can be created, deleted, renamed, and reordered.

use crate::core::id::{PlaylistId, TrackId};
use crate::database::Database;

/// Represents a playlist with an ordered list of tracks.
pub struct Playlist {
    pub id: PlaylistId,
    pub name: String,
    pub tracks: Vec<TrackId>,
}

/// Manages all playlists via the database.
pub struct PlaylistManager {
    db: Database,
}

impl PlaylistManager {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn create(&self, name: &str) -> anyhow::Result<PlaylistId> {
        let id = PlaylistId::new();
        let id_str = format!("{:?}", id);
        self.db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO playlists (id, name) VALUES (?1, ?2)",
                [&id_str, name],
            )?;
            Ok(())
        })?;
        Ok(id)
    }

    pub fn delete(&self, playlist_id: PlaylistId) -> anyhow::Result<()> {
        let id_str = format!("{:?}", playlist_id);
        self.db.with_connection(|conn| {
            conn.execute("DELETE FROM playlists WHERE id = ?1", [&id_str])?;
            Ok(())
        })
    }

    pub fn add_track(&self, playlist_id: PlaylistId, track_id: TrackId) -> anyhow::Result<()> {
        let pid_str = format!("{:?}", playlist_id);
        let tid_str = format!("{:?}", track_id);
        self.db.with_connection(|conn| {
            let pos: i64 = conn.query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
                [&pid_str],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
                rusqlite::params![&pid_str, &tid_str, pos],
            )?;
            conn.execute(
                "UPDATE playlists SET track_count = (SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?1), updated_at = datetime('now') WHERE id = ?1",
                [&pid_str],
            )?;
            Ok(())
        })
    }

    pub fn remove_track(&self, playlist_id: PlaylistId, track_id: TrackId) -> anyhow::Result<()> {
        let pid_str = format!("{:?}", playlist_id);
        let tid_str = format!("{:?}", track_id);
        self.db.with_connection(|conn| {
            conn.execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
                rusqlite::params![&pid_str, &tid_str],
            )?;
            conn.execute(
                "UPDATE playlists SET track_count = (SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?1), updated_at = datetime('now') WHERE id = ?1",
                [&pid_str],
            )?;
            Ok(())
        })
    }

    pub fn reorder(
        &self,
        playlist_id: PlaylistId,
        track_id: TrackId,
        new_position: u32,
    ) -> anyhow::Result<()> {
        let pid_str = format!("{:?}", playlist_id);
        let tid_str = format!("{:?}", track_id);
        self.db.with_connection(|conn| {
            conn.execute(
                "UPDATE playlist_tracks SET position = ?1 WHERE playlist_id = ?2 AND track_id = ?3",
                rusqlite::params![new_position, &pid_str, &tid_str],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use tempfile::TempDir;

    fn setup() -> (PlaylistManager, TempDir) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(&dir.path().join("test.db")).unwrap();
        (PlaylistManager::new(db), dir)
    }

    #[test]
    fn test_create_and_delete_playlist() {
        let (pm, _dir) = setup();
        let id = pm.create("Test Playlist").unwrap();
        // Delete should succeed
        pm.delete(id).unwrap();
    }

    #[test]
    fn test_playlist_track_management() {
        let (pm, _dir) = setup();
        let pid = pm.create("Test").unwrap();
        let tid = TrackId::new();
        let tid_str = format!("{:?}", tid);

        // Insert track first to satisfy FK constraint
        pm.db
            .with_connection(|conn| {
                conn.execute(
                    "INSERT INTO tracks (id, path, title) VALUES (?1, ?2, ?3)",
                    [&tid_str, "/fake/path.mp3", "Test Track"],
                )?;
                Ok(())
            })
            .unwrap();

        pm.add_track(pid, tid).unwrap();
        pm.remove_track(pid, tid).unwrap();
    }
}
