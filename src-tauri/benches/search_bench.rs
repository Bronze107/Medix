use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use medix::db;
use medix::search;
use std::path::PathBuf;
use tempfile::tempdir;

/// Create a test database with `n` media records, each with a random mix of tags.
fn seed_search_db(n: usize) -> (tempfile::TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("bench_search.db");
    db::setup_test_db(&db_path).unwrap();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    // Create tags
    for i in 0..10 {
        conn.execute(
            "INSERT INTO tags (id, name) VALUES (?1, ?2)",
            rusqlite::params![format!("t{}", i), format!("tag{}", i)],
        )
        .unwrap();
    }
    // Create media and assign random tags
    let now = "2026-01-01T00:00:00";
    for i in 0..n {
        let w = 100 + (i % 40) * 50;
        let h = 100 + (i % 30) * 60;
        let sz = 1024 + (i % 100) * 10000;
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![format!("m{}", i), format!("/tmp/m{}.jpg", i), w, h, sz, now],
        )
        .unwrap();
        // Assign 1–3 tags randomly, ensuring tag:cat is on ~30% of media
        if i % 3 == 0 {
            conn.execute(
                "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?1, 't0')",
                rusqlite::params![format!("m{}", i)],
            )
            .unwrap();
        }
        if i % 2 == 0 {
            conn.execute(
                "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?1, 't1')",
                rusqlite::params![format!("m{}", i)],
            )
            .unwrap();
        }
    }
    (dir, db_path)
}

fn bench_search(c: &mut Criterion) {
    for &n in &[100, 1_000, 10_000] {
        let (_dir, db_path) = seed_search_db(n);

        c.bench_with_input(
            BenchmarkId::new("execute_search_path empty", n),
            &db_path,
            |b, db_path| {
                b.iter(|| {
                    search::execute_search_path(
                        black_box(db_path),
                        black_box(""),
                        black_box("imported_at"),
                        black_box(true),
                        black_box(false),
                    )
                })
            },
        );

        c.bench_with_input(
            BenchmarkId::new("execute_search_path tag:cat", n),
            &db_path,
            |b, db_path| {
                b.iter(|| {
                    search::execute_search_path(
                        black_box(db_path),
                        black_box("tag:cat"),
                        black_box("imported_at"),
                        black_box(true),
                        black_box(false),
                    )
                })
            },
        );

        c.bench_with_input(
            BenchmarkId::new("execute_search_path tag:cat width:>5000 size:>100kb", n),
            &db_path,
            |b, db_path| {
                b.iter(|| {
                    search::execute_search_path(
                        black_box(db_path),
                        black_box("tag:cat width:>5000 size:>100kb"),
                        black_box("imported_at"),
                        black_box(true),
                        black_box(false),
                    )
                })
            },
        );
    }
}

fn bench_find_items_with_tags(c: &mut Criterion) {
    for &n in &[100, 1_000, 10_000] {
        let (_dir, db_path) = seed_search_db(n);

        // Build BrowseItem list from the seeded DB
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let mut stmt = conn
            .prepare("SELECT id, width, height, file_size, imported_at, media_type FROM media")
            .unwrap();
        let items: Vec<medix::media::BrowseItem> = stmt
            .query_map([], |row| {
                Ok(medix::media::BrowseItem {
                    item_id: row.get(0)?,
                    media_id: row.get(0)?,
                    item_kind: "original".into(),
                    variant_id: None,
                    is_display_variant: false,
                    source_path: Some("/tmp/x.jpg".into()),
                    width: row.get(1)?,
                    height: row.get(2)?,
                    file_size: row.get(3)?,
                    created_at: None,
                    modified_at: None,
                    imported_at: row.get(4)?,
                    source_url: None,
                    page_url: None,
                    source: None,
                    sha256: None,
                    deleted_at: None,
                    display_variant_id: None,
                    thumb_256: None,
                    lqip: None,
                    media_type: row.get(5)?,
                    duration: None,
                    video_codec: None,
                    video_fps: None,
                    label: None,
                    preset_name: None,
                })
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let tag_names = vec!["tag0".to_string(), "tag1".to_string()];

        c.bench_with_input(
            BenchmarkId::new("find_items_with_tags_path (union)", n),
            &(&db_path, &items, &tag_names),
            |b, (db_path, items, tag_names)| {
                b.iter(|| {
                    db::find_items_with_tags_path(
                        black_box(db_path),
                        black_box(items),
                        black_box(tag_names),
                    )
                })
            },
        );
    }
}

criterion_group!(search_benches, bench_search, bench_find_items_with_tags);
criterion_main!(search_benches);
