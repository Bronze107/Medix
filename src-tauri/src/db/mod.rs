use rusqlite::{params, Connection};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use ulid::Ulid;

use crate::media::Media;
use crate::tag::Tag;
use crate::variants::Variant;

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

        INSERT OR IGNORE INTO _migrations (name) VALUES ('0002_tags');

        CREATE TABLE IF NOT EXISTS tags (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS media_tags (
            media_id TEXT NOT NULL,
            tag_id TEXT NOT NULL,
            confidence REAL,
            source TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (media_id, tag_id),
            FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
            FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_media_tags_media ON media_tags(media_id);
        CREATE INDEX IF NOT EXISTS idx_media_tags_tag ON media_tags(tag_id);

        INSERT OR IGNORE INTO _migrations (name) VALUES ('0003_variants');

        CREATE TABLE IF NOT EXISTS variants (
            id TEXT PRIMARY KEY,
            media_id TEXT NOT NULL,
            preset_name TEXT NOT NULL,
            format TEXT NOT NULL,
            width INTEGER,
            height INTEGER,
            quality INTEGER,
            file_size INTEGER,
            file_path TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_variants_media ON variants(media_id);
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

// --- Tag operations ---

pub fn tag_list(app: &AppHandle) -> Result<Vec<Tag>, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    let mut stmt = conn.prepare("SELECT id, name FROM tags ORDER BY name")?;
    let tag_iter = stmt.query_map([], |row| {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    let mut results = Vec::new();
    for tag in tag_iter {
        results.push(tag?);
    }
    Ok(results)
}

pub fn tag_create(
    app: &AppHandle,
    name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    let id = Ulid::new().to_string();
    let name_lower = name.to_lowercase();
    conn.execute(
        "INSERT INTO tags (id, name) VALUES (?1, ?2)",
        params![&id, &name_lower],
    )?;
    Ok(id)
}

pub fn tag_delete(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn tag_rename(
    app: &AppHandle,
    id: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    let name_lower = name.to_lowercase();
    conn.execute(
        "UPDATE tags SET name = ?1 WHERE id = ?2",
        params![&name_lower, id],
    )?;
    Ok(())
}

pub fn media_tags_get(
    app: &AppHandle,
    media_id: &str,
) -> Result<Vec<Tag>, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name FROM tags t
         JOIN media_tags mt ON t.id = mt.tag_id
         WHERE mt.media_id = ?1
         ORDER BY t.name",
    )?;
    let tag_iter = stmt.query_map(params![media_id], |row| {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    let mut results = Vec::new();
    for tag in tag_iter {
        results.push(tag?);
    }
    Ok(results)
}

pub fn media_tag_add(
    app: &AppHandle,
    media_id: &str,
    tag_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    conn.execute(
        "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?1, ?2)",
        params![media_id, tag_id],
    )?;
    Ok(())
}

pub fn media_tag_add_batch(
    app: &AppHandle,
    media_ids: &[String],
    tag_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    for media_id in media_ids {
        conn.execute(
            "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?1, ?2)",
            params![media_id, tag_id],
        )?;
    }
    Ok(())
}

pub fn media_tag_remove(
    app: &AppHandle,
    media_id: &str,
    tag_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    conn.execute(
        "DELETE FROM media_tags WHERE media_id = ?1 AND tag_id = ?2",
        params![media_id, tag_id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum TagSearchMode {
    Intersection,
    Union,
}

pub fn media_search_by_tags(
    app: &AppHandle,
    tag_names: &[String],
    sort_by: &str,
    descending: bool,
    mode: TagSearchMode,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    if tag_names.is_empty() {
        return list_media(app, sort_by, descending);
    }

    let path = db_path(app);
    let conn = Connection::open(&path)?;

    let order = if descending { "DESC" } else { "ASC" };
    let sort_column = match sort_by {
        "created_at" => "created_at",
        "modified_at" => "modified_at",
        _ => "imported_at",
    };

    let placeholders = tag_names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = match mode {
        TagSearchMode::Intersection => format!(
            "SELECT m.id, m.source_path, m.width, m.height, m.file_size, m.created_at, m.modified_at, m.imported_at
             FROM media m
             JOIN media_tags mt ON m.id = mt.media_id
             JOIN tags t ON mt.tag_id = t.id
             WHERE t.name IN ({})
             GROUP BY m.id
             HAVING COUNT(DISTINCT t.id) = {}
             ORDER BY m.{} {}",
            placeholders, tag_names.len(), sort_column, order
        ),
        TagSearchMode::Union => format!(
            "SELECT DISTINCT m.id, m.source_path, m.width, m.height, m.file_size, m.created_at, m.modified_at, m.imported_at
             FROM media m
             JOIN media_tags mt ON m.id = mt.media_id
             JOIN tags t ON mt.tag_id = t.id
             WHERE t.name IN ({})
             ORDER BY m.{} {}",
            placeholders, sort_column, order
        ),
    };

    let mut stmt = conn.prepare(&sql)?;
    let params_vec: Vec<&dyn rusqlite::ToSql> = tag_names
        .iter()
        .map(|n| n as &dyn rusqlite::ToSql)
        .collect();
    let media_iter = stmt.query_map(params_vec.as_slice(), |row| {
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

// --- Variant operations ---

pub fn variant_list(
    app: &AppHandle,
    media_id: &str,
) -> Result<Vec<Variant>, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    let mut stmt = conn.prepare(
        "SELECT id, media_id, preset_name, format, width, height, quality, file_size, file_path
         FROM variants WHERE media_id = ?1 ORDER BY created_at",
    )?;
    let variant_iter = stmt.query_map(params![media_id], |row| {
        Ok(Variant {
            id: row.get(0)?,
            media_id: row.get(1)?,
            preset_name: row.get(2)?,
            format: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            quality: row.get(6)?,
            file_size: row.get(7)?,
            file_path: row.get(8)?,
        })
    })?;
    let mut results = Vec::new();
    for v in variant_iter {
        results.push(v?);
    }
    Ok(results)
}

pub fn variant_insert(
    app: &AppHandle,
    variant: &Variant,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    conn.execute(
        "INSERT OR REPLACE INTO variants (id, media_id, preset_name, format, width, height, quality, file_size, file_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            &variant.id,
            &variant.media_id,
            &variant.preset_name,
            &variant.format,
            variant.width,
            variant.height,
            variant.quality,
            variant.file_size,
            &variant.file_path,
        ],
    )?;
    Ok(())
}

pub fn variant_delete(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    conn.execute("DELETE FROM variants WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn variant_get_by_id(
    app: &AppHandle,
    id: &str,
) -> Result<Option<Variant>, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    let mut stmt = conn.prepare(
        "SELECT id, media_id, preset_name, format, width, height, quality, file_size, file_path
         FROM variants WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(params![id], |row| {
        Ok(Variant {
            id: row.get(0)?,
            media_id: row.get(1)?,
            preset_name: row.get(2)?,
            format: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            quality: row.get(6)?,
            file_size: row.get(7)?,
            file_path: row.get(8)?,
        })
    })?;
    if let Some(row) = rows.next() {
        return Ok(Some(row?));
    }
    Ok(None)
}

pub fn variant_get_by_media_and_preset(
    app: &AppHandle,
    media_id: &str,
    preset_name: &str,
) -> Result<Option<Variant>, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let conn = Connection::open(&path)?;
    let mut stmt = conn.prepare(
        "SELECT id, media_id, preset_name, format, width, height, quality, file_size, file_path
         FROM variants WHERE media_id = ?1 AND preset_name = ?2",
    )?;
    let mut rows = stmt.query_map(params![media_id, preset_name], |row| {
        Ok(Variant {
            id: row.get(0)?,
            media_id: row.get(1)?,
            preset_name: row.get(2)?,
            format: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            quality: row.get(6)?,
            file_size: row.get(7)?,
            file_path: row.get(8)?,
        })
    })?;
    if let Some(row) = rows.next() {
        return Ok(Some(row?));
    }
    Ok(None)
}
