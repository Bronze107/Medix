use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

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

        -- Phase 1: Initial schema
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
