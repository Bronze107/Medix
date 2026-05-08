use rusqlite::{params, Connection};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::media::Media;

pub fn db_path(app: &AppHandle) -> PathBuf {
    let app_dir = app.path().app_data_dir().expect("Failed to get app data dir");
    fs::create_dir_all(&app_dir).expect("Failed to create app data dir");
    app_dir.join("medix.db")
}

pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path(app);
    let mut conn = Connection::open(&path)?;
    run_migrations(&mut conn)?;
    Ok(())
}

fn run_migrations(conn: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        INSERT OR IGNORE INTO _migrations (name) VALUES ('0001_initial');

        CREATE TABLE IF NOT EXISTS media (
            id TEXT PRIMARY KEY,
            source_path TEXT,
            phash BLOB,
            width INTEGER,
            height INTEGER,
            file_size INTEGER,
            created_at TIMESTAMP,
            modified_at TIMESTAMP,
            imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_media_imported_at ON media(imported_at);
        CREATE INDEX IF NOT EXISTS idx_media_created_at ON media(created_at);
        ",
    )?;
    Ok(())
}

pub fn insert_media(app: &AppHandle, media: &Media) -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    conn.execute(
        "INSERT INTO media (id, source_path, width, height, file_size, created_at, modified_at, imported_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            &media.id,
            media.source_path.as_ref(),
            media.width,
            media.height,
            media.file_size,
            media.created_at.as_ref(),
            media.modified_at.as_ref(),
            &media.imported_at,
        ],
    )?;
    Ok(())
}

fn resolve_thumb_paths(app: &AppHandle, media_list: &mut [Media]) {
    let Ok(app_dir) = app.path().app_data_dir() else { return };
    let thumbs_dir = app_dir.join("thumbnails");

    for media in media_list {
        let thumb_256 = thumbs_dir.join(format!("{}_256.jpg", media.id));
        if thumb_256.exists() {
            media.thumb_256 = Some(thumb_256.to_string_lossy().replace('\\', "/"));
        }
        let thumb_512 = thumbs_dir.join(format!("{}_512.jpg", media.id));
        if thumb_512.exists() {
            media.thumb_512 = Some(thumb_512.to_string_lossy().replace('\\', "/"));
        }
    }
}

pub fn list_media(
    app: &AppHandle,
    sort_by: &str,
    descending: bool,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;

    let order = if descending { "DESC" } else { "ASC" };
    let sort_column = match sort_by {
        "created_at" => "created_at",
        "modified_at" => "modified_at",
        _ => "imported_at",
    };

    let sql = format!(
        "SELECT id, source_path, width, height, file_size, created_at, modified_at, imported_at
         FROM media
         ORDER BY {} {}",
        sort_column, order
    );

    let mut stmt = conn.prepare(&sql)?;
    let media_iter = stmt.query_map([], |row| {
        Ok(Media {
            id: row.get(0)?,
            source_path: row.get(1)?,
            width: row.get(2)?,
            height: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
            modified_at: row.get(6)?,
            imported_at: row.get(7)?,
            thumb_256: None,
            thumb_512: None,
        })
    })?;

    let mut results = Vec::new();
    for media in media_iter {
        results.push(media?);
    }

    resolve_thumb_paths(app, &mut results);

    Ok(results)
}
