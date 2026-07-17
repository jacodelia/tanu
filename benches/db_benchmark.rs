//! Performance benchmarks for Tanu.
//!
//! Run with: cargo bench
//! Or for a specific benchmark: cargo bench -- db_insert_100k

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

use tanu::database::Database;

fn setup_db(track_count: usize) -> (TempDir, Database, PathBuf) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("bench.db");
    let db = Database::open(&db_path).unwrap();

    db.with_connection(|conn| {
        for i in 0..track_count {
            let artist_id = format!("artist-{}", i % 1000);
            let album_id = format!("album-{}", i % 5000);
            let track_id = format!("track-{}", i);
            let tkn = (i % 15 + 1).to_string();
            let dur = "240.0".to_string();
            let fmt_s = "mp3".to_string();

            conn.execute(
                "INSERT OR IGNORE INTO artists (id, name) VALUES (?1, ?2)",
                [&artist_id as &dyn rusqlite::types::ToSql, &format!("Artist {}", i % 1000) as &dyn rusqlite::types::ToSql],
            ).ok();

            conn.execute(
                "INSERT OR IGNORE INTO albums (id, title, artist_id, year) VALUES (?1, ?2, ?3, ?4)",
                [
                    &album_id as &dyn rusqlite::types::ToSql,
                    &format!("Album {}", i % 5000) as &dyn rusqlite::types::ToSql,
                    &artist_id as &dyn rusqlite::types::ToSql,
                    &"2021" as &dyn rusqlite::types::ToSql,
                ],
            ).ok();

            conn.execute(
                "INSERT INTO tracks (id, path, title, artist_id, album_id, track_number, duration_secs, format)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                [
                    &track_id as &dyn rusqlite::types::ToSql,
                    &format!("/music/track{}.mp3", i) as &dyn rusqlite::types::ToSql,
                    &format!("Track {}", i) as &dyn rusqlite::types::ToSql,
                    &artist_id as &dyn rusqlite::types::ToSql,
                    &album_id as &dyn rusqlite::types::ToSql,
                    &tkn as &dyn rusqlite::types::ToSql,
                    &dur as &dyn rusqlite::types::ToSql,
                    &fmt_s as &dyn rusqlite::types::ToSql,
                ],
            ).unwrap();
        }
        Ok(())
    }).unwrap();

    (dir, db, db_path)
}

fn bench_db_insert_1k(c: &mut Criterion) {
    let mut group = c.benchmark_group("db_insert");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("insert_1000_tracks", |b| {
        b.iter_batched(
            || {
                let dir = TempDir::new().unwrap();
                let db_path = dir.path().join("bench.db");
                let db = Database::open(&db_path).unwrap();
                (dir, db)
            },
            |(_dir, db)| {
                db.with_connection(|conn| {
                    for i in 0..1000 {
                        let artist_id = format!("artist-{}", i % 50);
                        let artist_name = format!("Artist {}", i % 50);
                        let album_id = format!("album-{}", i % 200);
                        let album_name = format!("Album {}", i % 200);
                        let track_id = format!("track-{}", i);
                        let path = format!("/music/track{}.mp3", i);
                        let track_name = format!("Track {}", i);
                        let tkn = (i % 12 + 1).to_string();
                        let dur = "200.0".to_string();
                        let fmt_s = "mp3".to_string();

                        conn.execute(
                            "INSERT OR IGNORE INTO artists (id, name) VALUES (?1, ?2)",
                            [&artist_id as &dyn rusqlite::types::ToSql, &artist_name as &dyn rusqlite::types::ToSql],
                        ).ok();
                        conn.execute(
                            "INSERT OR IGNORE INTO albums (id, title, artist_id) VALUES (?1, ?2, ?3)",
                            [&album_id as &dyn rusqlite::types::ToSql, &album_name as &dyn rusqlite::types::ToSql, &artist_id as &dyn rusqlite::types::ToSql],
                        ).ok();
                        conn.execute(
                            "INSERT INTO tracks (id, path, title, artist_id, album_id, track_number, duration_secs, format)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            [
                                &track_id as &dyn rusqlite::types::ToSql,
                                &path as &dyn rusqlite::types::ToSql,
                                &track_name as &dyn rusqlite::types::ToSql,
                                &artist_id as &dyn rusqlite::types::ToSql,
                                &album_id as &dyn rusqlite::types::ToSql,
                                &tkn as &dyn rusqlite::types::ToSql,
                                &dur as &dyn rusqlite::types::ToSql,
                                &fmt_s as &dyn rusqlite::types::ToSql,
                            ],
                        ).unwrap();
                    }
                    Ok(())
                }).unwrap();
            },
            criterion::BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn bench_db_query_artists(c: &mut Criterion) {
    let (_dir, db, _path) = setup_db(100_000);

    c.bench_function("query_all_artists_100k_tracks", |b| {
        b.iter(|| {
            db.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT a.id, a.name FROM artists a
                     WHERE EXISTS (SELECT 1 FROM tracks t WHERE t.artist_id = a.id)
                     ORDER BY a.name",
                    )
                    .unwrap();
                let results: Vec<(String, String)> = stmt
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                black_box(results.len());
                Ok(())
            })
            .unwrap();
        });
    });
}

fn bench_db_query_albums(c: &mut Criterion) {
    let (_dir, db, _path) = setup_db(100_000);

    c.bench_function("query_albums_for_artist_100k", |b| {
        b.iter(|| {
            db.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT al.id, al.title FROM albums al
                     WHERE al.artist_id = ?1
                     ORDER BY al.year, al.title",
                    )
                    .unwrap();
                let results: Vec<(String, String)> = stmt
                    .query_map(["artist-42"], |row| Ok((row.get(0)?, row.get(1)?)))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                black_box(results.len());
                Ok(())
            })
            .unwrap();
        });
    });
}

fn bench_db_query_fts(c: &mut Criterion) {
    let (_dir, db, _path) = setup_db(100_000);

    c.bench_function("fts_search_100k", |b| {
        b.iter(|| {
            db.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT t.path FROM tracks_fts fts
                     JOIN tracks t ON t.rowid = fts.rowid
                     WHERE tracks_fts MATCH ?1
                     LIMIT 100",
                    )
                    .unwrap();
                let results: Vec<String> = stmt
                    .query_map(["Track 5*"], |row| row.get(0))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                black_box(results.len());
                Ok(())
            })
            .unwrap();
        });
    });
}

fn bench_scroll_virtual(c: &mut Criterion) {
    let (_dir, _db, _path) = setup_db(100_000);

    // Simulate virtual scroll: read a window of 30 rows from a large offset
    c.bench_function("scroll_100k_list_slice_30", |b| {
        b.iter(|| {
            // Simulate: slice 30 rows from position 50000
            let start: usize = 50000;
            let end: usize = (start + 30).min(100_000);
            black_box(start..end);
        });
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench_db_insert_1k, bench_db_query_artists, bench_db_query_albums,
              bench_db_query_fts, bench_scroll_virtual
);
criterion_main!(benches);
