use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use ulid::Ulid;

use crate::captions::Caption;
use crate::media::Media;
use crate::tag::Tag;
use crate::variants::Variant;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Initialize the connection pool and return it for Tauri managed state.
pub fn init_pool(app: &AppHandle) -> DbPool {
    let path = db_path(app);
    let manager = SqliteConnectionManager::file(&path);
    Pool::builder()
        .max_size(4)
        .build(manager)
        .expect("failed to build DB connection pool")
}

pub fn db_path(app: &AppHandle) -> PathBuf {
    let app_dir = app.path().app_data_dir().expect("Failed to get app data dir");
    fs::create_dir_all(&app_dir).expect("Failed to create app data dir");
    app_dir.join("medix.db")
}

/// Get a pooled connection from the Tauri managed state.
fn get_conn(app: &AppHandle) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, String> {
    app.state::<DbPool>().get().map_err(|e| e.to_string())
}

/// Standalone DB path for CLI / testing (no Tauri AppHandle required).
pub fn db_path_standalone() -> PathBuf {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
    } else {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".local").join("share")
    };
    let app_dir = base.join("com.bronze107.medix");
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

        INSERT OR IGNORE INTO _migrations (name) VALUES ('0004_captions');

        CREATE TABLE IF NOT EXISTS captions (
            id TEXT PRIMARY KEY,
            media_id TEXT NOT NULL,
            text TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_captions_media ON captions(media_id);

        INSERT OR IGNORE INTO _migrations (name) VALUES ('0005_embeddings');

        CREATE TABLE IF NOT EXISTS embeddings (
            media_id TEXT NOT NULL,
            model TEXT NOT NULL,
            content_type TEXT NOT NULL,
            vector BLOB NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (media_id, model, content_type),
            FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_embeddings_media ON embeddings(media_id);

        INSERT OR IGNORE INTO _migrations (name) VALUES ('0006_settings');

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        ",
    )?;

    // 0007: add source column to captions (conditional — SQLite can't do IF NOT EXISTS on ALTER TABLE)
    let has_source: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('captions') WHERE name='source'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !has_source {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0007_ai_fields');
             ALTER TABLE captions ADD COLUMN source TEXT;",
        )?;
    }

    // 0008: media source tracking
    let has_source_url: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('media') WHERE name='source_url'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !has_source_url {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0008_media_source');
             ALTER TABLE media ADD COLUMN source_url TEXT;
             ALTER TABLE media ADD COLUMN page_url TEXT;
             ALTER TABLE media ADD COLUMN source TEXT;",
        )?;
    }

    // 0009: soft delete
    let has_deleted_at: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('media') WHERE name='deleted_at'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !has_deleted_at {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0009_soft_delete');
             ALTER TABLE media ADD COLUMN deleted_at TEXT;",
        )?;
    }

    // 0010: sha256 for dedup
    let has_sha256: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('media') WHERE name='sha256'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !has_sha256 {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0010_sha256');
             ALTER TABLE media ADD COLUMN sha256 TEXT;",
        )?;
    }

    // 0011: variant versioning — add label and source columns
    let has_label: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('variants') WHERE name='label'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !has_label {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0011_variant_versioning');
             ALTER TABLE variants ADD COLUMN label TEXT;
             ALTER TABLE variants ADD COLUMN source TEXT DEFAULT 'generated';",
        )?;
        // Backfill existing preset-based variants with Chinese labels
        conn.execute(
            "UPDATE variants SET label = 'Web分享' WHERE preset_name = 'web_share' AND label IS NULL",
            [],
        )?;
        conn.execute(
            "UPDATE variants SET label = '打印' WHERE preset_name = 'print' AND label IS NULL",
            [],
        )?;
        conn.execute(
            "UPDATE variants SET label = '训练数据集' WHERE preset_name = 'dataset' AND label IS NULL",
            [],
        )?;
    }

    // 0012: collections
    let has_collections: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='collections'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !has_collections {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0012_collections');
             CREATE TABLE IF NOT EXISTS collections (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 description TEXT,
                 pinned_at TEXT,
                 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS collection_items (
                 collection_id TEXT NOT NULL,
                 media_id TEXT NOT NULL,
                 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                 PRIMARY KEY (collection_id, media_id),
                 FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
                 FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
             );
             CREATE INDEX IF NOT EXISTS idx_collection_items_cid ON collection_items(collection_id);
             CREATE INDEX IF NOT EXISTS idx_collection_items_mid ON collection_items(media_id);",
        )?;
    }

    // 0013: variant annotation + display variant
    let has_variant_captions: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('captions') WHERE name='variant_id'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !has_variant_captions {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0013_variant_annotation');
             ALTER TABLE captions ADD COLUMN variant_id TEXT REFERENCES variants(id) ON DELETE CASCADE;
             ALTER TABLE embeddings ADD COLUMN variant_id TEXT REFERENCES variants(id) ON DELETE CASCADE;
             ALTER TABLE media ADD COLUMN display_variant_id TEXT REFERENCES variants(id) ON DELETE SET NULL;
             CREATE INDEX IF NOT EXISTS idx_captions_variant ON captions(variant_id);
             CREATE INDEX IF NOT EXISTS idx_embeddings_variant ON embeddings(variant_id);",
        )?;
    }

    // 0014: variant tags
    let has_variant_tags: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('media_tags') WHERE name='variant_id'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !has_variant_tags {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0014_variant_tags');
             ALTER TABLE media_tags ADD COLUMN variant_id TEXT REFERENCES variants(id) ON DELETE CASCADE;
             CREATE INDEX IF NOT EXISTS idx_media_tags_variant ON media_tags(variant_id);",
        )?;
    }

    // --- 0015_performance_indexes ---
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0015_performance_indexes'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0015_performance_indexes');
                 CREATE INDEX IF NOT EXISTS idx_media_sha256 ON media(sha256);
                 CREATE INDEX IF NOT EXISTS idx_media_deleted_at ON media(deleted_at);
                 CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model);",
            )?;
        }
    }

    // --- 0016_lqip ---
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0016_lqip'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0016_lqip');
                 ALTER TABLE media ADD COLUMN lqip TEXT;",
            )?;
        }
    }

    // --- 0017_fts5 ---
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0017_fts5'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0017_fts5');
                 CREATE VIRTUAL TABLE IF NOT EXISTS media_fts USING fts5(
                     media_id UNINDEXED,
                     search_text,
                     tokenize='unicode61 remove_diacritics 1'
                 );",
            )?;
        }
    }

    Ok(())
}

// --- Collection operations ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub pinned_at: Option<String>,
    pub created_at: String,
    pub item_count: Option<i64>,
}

pub fn collection_list(app: &AppHandle) -> Result<Vec<Collection>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.description, c.pinned_at, c.created_at,
                (SELECT COUNT(*) FROM collection_items ci
                 JOIN media m ON ci.media_id = m.id
                 WHERE ci.collection_id = c.id AND m.deleted_at IS NULL) as item_count
         FROM collections c ORDER BY c.pinned_at IS NULL, c.pinned_at, c.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Collection {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            pinned_at: row.get(3)?,
            created_at: row.get(4)?,
            item_count: row.get(5)?,
        })
    })?;
    let mut results = Vec::new();
    for r in rows { results.push(r?); }
    Ok(results)
}

pub fn collection_get(app: &AppHandle, id: &str) -> Result<Option<Collection>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.description, c.pinned_at, c.created_at,
                (SELECT COUNT(*) FROM collection_items ci
                 JOIN media m ON ci.media_id = m.id
                 WHERE ci.collection_id = c.id AND m.deleted_at IS NULL) as item_count
         FROM collections c WHERE c.id = ?1",
    )?;
    let mut rows = stmt.query_map(params![id], |row| {
        Ok(Collection {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            pinned_at: row.get(3)?,
            created_at: row.get(4)?,
            item_count: row.get(5)?,
        })
    })?;
    if let Some(r) = rows.next() { return Ok(Some(r?)); }
    Ok(None)
}

pub fn collection_create(app: &AppHandle, name: &str, description: &str) -> Result<String, Box<dyn std::error::Error>> {
    let id = ulid::Ulid::new().to_string();
    let desc = if description.is_empty() { None } else { Some(description.to_string()) };
    let conn = get_conn(app)?;
    conn.execute(
        "INSERT INTO collections (id, name, description) VALUES (?1, ?2, ?3)",
        params![&id, name, desc],
    )?;
    Ok(id)
}

pub fn collection_delete(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn collection_rename(app: &AppHandle, id: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute("UPDATE collections SET name = ?2 WHERE id = ?1", params![id, name])?;
    Ok(())
}

pub fn collection_pin(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute("UPDATE collections SET pinned_at = datetime('now') WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn collection_unpin(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute("UPDATE collections SET pinned_at = NULL WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn collection_add_item(app: &AppHandle, collection_id: &str, media_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "INSERT OR IGNORE INTO collection_items (collection_id, media_id) VALUES (?1, ?2)",
        params![collection_id, media_id],
    )?;
    Ok(())
}

pub fn collection_add_batch(app: &AppHandle, collection_id: &str, media_ids: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute("BEGIN TRANSACTION", [])?;
    let result = (|| {
        for mid in media_ids {
            conn.execute(
                "INSERT OR IGNORE INTO collection_items (collection_id, media_id) VALUES (?1, ?2)",
                params![collection_id, mid],
            )?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => { conn.execute("COMMIT", [])?; Ok(()) }
        Err(e) => { let _ = conn.execute("ROLLBACK", []); Err(e) }
    }
}

pub fn collection_remove_item(app: &AppHandle, collection_id: &str, media_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "DELETE FROM collection_items WHERE collection_id = ?1 AND media_id = ?2",
        params![collection_id, media_id],
    )?;
    Ok(())
}

pub fn media_list_by_collection(
    app: &AppHandle,
    collection_id: &str,
    sort_by: &str,
    descending: bool,
    offset: u32,
    limit: u32,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let order = if descending { "DESC" } else { "ASC" };
    let sort_column = match sort_by {
        "created_at" => "m.created_at",
        "modified_at" => "m.modified_at",
        "file_size" => "m.file_size",
        "width" => "m.width",
        "height" => "m.height",
        _ => "m.imported_at",
    };
    let sql = format!(
        "SELECT m.id, m.source_path, m.width, m.height, m.file_size,
                m.created_at, m.modified_at, m.imported_at,
                m.source_url, m.page_url, m.source, m.sha256, m.deleted_at,
                m.display_variant_id, m.lqip
         FROM media m
         JOIN collection_items ci ON ci.media_id = m.id
         WHERE ci.collection_id = ?1 AND m.deleted_at IS NULL
         ORDER BY {} {}
         LIMIT ?2 OFFSET ?3",
        sort_column, order
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![collection_id, limit as i64, offset as i64], |row| {
        Ok(Media {
            id: row.get(0)?,
            source_path: row.get(1)?,
            phash: None,
            width: row.get(2)?,
            height: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
            modified_at: row.get(6)?,
            imported_at: row.get(7)?,
            source_url: row.get(8)?,
            page_url: row.get(9)?,
            source: row.get(10)?,
            sha256: row.get(11)?,
            deleted_at: row.get(12)?,
            display_variant_id: row.get(13)?,
            lqip: row.get(14)?,
            thumb_256: None,
        })
    })?;
    let mut results = Vec::new();
    for r in rows { results.push(r?); }
    Ok(results)
}

pub fn collection_get_item_ids(app: &AppHandle, collection_id: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare("SELECT media_id FROM collection_items WHERE collection_id = ?1")?;
    let rows = stmt.query_map(params![collection_id], |row| row.get::<_, String>(0))?;
    let mut results = Vec::new();
    for r in rows { results.push(r?); }
    Ok(results)
}

pub fn collection_first_media_id(app: &AppHandle, collection_id: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare("SELECT media_id FROM collection_items WHERE collection_id = ?1 ORDER BY created_at LIMIT 1")?;
    let mut rows = stmt.query_map(params![collection_id], |row| row.get::<_, String>(0))?;
    if let Some(r) = rows.next() { return Ok(Some(r?)); }
    Ok(None)
}

pub fn media_get_by_sha256(
    app: &AppHandle,
    hash: &str,
) -> Result<Option<Media>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT id, source_path, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, deleted_at, display_variant_id, lqip
         FROM media WHERE sha256 = ?1 AND deleted_at IS NULL LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![hash], |row| {
        Ok(Media {
            id: row.get(0)?,
            source_path: row.get(1)?,
            phash: None,
            width: row.get(2)?,
            height: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
            modified_at: row.get(6)?,
            imported_at: row.get(7)?,
            source_url: row.get(8)?,
            page_url: row.get(9)?,
            source: row.get(10)?,
            sha256: row.get(11)?,
            deleted_at: row.get(12)?,
            display_variant_id: row.get(13)?,
            lqip: row.get(14)?,
            thumb_256: None,
        })
    })?;
    if let Some(row) = rows.next() {
        return Ok(Some(row?));
    }
    Ok(None)
}

pub fn insert_media(app: &AppHandle, media: &Media) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "INSERT INTO media (id, source_path, phash, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, lqip)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            &media.id,
            media.source_path.as_ref(),
            media.phash.as_ref(),
            media.width,
            media.height,
            media.file_size,
            media.created_at.as_ref(),
            media.modified_at.as_ref(),
            &media.imported_at,
            media.source_url.as_ref(),
            media.page_url.as_ref(),
            media.source.as_ref(),
            media.sha256.as_ref(),
            media.lqip.as_ref(),
        ],
    )?;
    Ok(())
}

pub(crate) fn resolve_thumb_paths(app: &AppHandle, media_list: &mut [Media]) {
    let Ok(app_dir) = app.path().app_data_dir() else { return };
    let thumbs_dir = app_dir.join("thumbnails");

    // Always set the expected path — thumbnails are generated synchronously during import.
    // Frontend useThumbnail hook handles missing files with retry/fallback.
    for media in media_list {
        media.thumb_256 = Some(
            thumbs_dir
                .join(format!("{}_256.jpg", media.id))
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
}

pub fn list_media_path(
    db_path: &Path,
    sort_by: &str,
    descending: bool,
    offset: u32,
    limit: u32,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    let conn = Connection::open(db_path)?;

    let order = if descending { "DESC" } else { "ASC" };
    let sort_column = match sort_by {
        "created_at" => "created_at",
        "modified_at" => "modified_at",
        "file_size" => "file_size",
        "width" => "width",
        "height" => "height",
        _ => "imported_at",
    };

    let sql = format!(
        "SELECT id, source_path, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, deleted_at, display_variant_id, lqip
         FROM media
         WHERE deleted_at IS NULL
         ORDER BY {} {}
         LIMIT ? OFFSET ?",
        sort_column, order
    );

    let mut stmt = conn.prepare(&sql)?;
    let media_iter = stmt.query_map(params![limit as i64, offset as i64], |row| {
        Ok(Media {
            id: row.get(0)?,
            source_path: row.get(1)?,
            phash: None,
            width: row.get(2)?,
            height: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
            modified_at: row.get(6)?,
            imported_at: row.get(7)?,
            source_url: row.get(8)?,
            page_url: row.get(9)?,
            source: row.get(10)?,
            sha256: row.get(11)?,
            deleted_at: row.get(12)?,
            display_variant_id: row.get(13)?,
            lqip: row.get(14)?,
            thumb_256: None,
        })
    })?;

    let mut results = Vec::new();
    for media in media_iter {
        results.push(media?);
    }

    Ok(results)
}

pub fn list_media(
    app: &AppHandle,
    sort_by: &str,
    descending: bool,
    offset: u32,
    limit: u32,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let mut results = list_media_path(&path, sort_by, descending, offset, limit)?;
    resolve_thumb_paths(app, &mut results);
    Ok(results)
}

pub fn list_media_count(app: &AppHandle) -> Result<usize, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

pub fn media_get_batch(
    app: &AppHandle,
    ids: &[String],
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let conn = get_conn(app)?;
    let placeholders: Vec<String> = (0..ids.len()).map(|i| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT id, source_path, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, deleted_at, display_variant_id, lqip
         FROM media WHERE deleted_at IS NULL AND id IN ({})",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
    let iter = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(Media {
            id: row.get(0)?,
            source_path: row.get(1)?,
            phash: None,
            width: row.get(2)?,
            height: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
            modified_at: row.get(6)?,
            imported_at: row.get(7)?,
            source_url: row.get(8)?,
            page_url: row.get(9)?,
            source: row.get(10)?,
            sha256: row.get(11)?,
            deleted_at: row.get(12)?,
            display_variant_id: row.get(13)?,
            lqip: row.get(14)?,
            thumb_256: None,
        })
    })?;
    let mut results = Vec::new();
    for r in iter {
        results.push(r?);
    }
    Ok(results)
}

// --- Tag operations ---

pub fn tag_list_path(db_path: &Path) -> Result<Vec<Tag>, Box<dyn std::error::Error>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, COUNT(m.id) as item_count
         FROM tags t
         LEFT JOIN media_tags mt ON t.id = mt.tag_id
         LEFT JOIN media m ON mt.media_id = m.id AND m.deleted_at IS NULL
         GROUP BY t.id
         ORDER BY t.name"
    )?;
    let tag_iter = stmt.query_map([], |row| {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            source: None,
            confidence: None,
            item_count: Some(row.get(2)?),
        })
    })?;
    let mut results = Vec::new();
    for tag in tag_iter {
        results.push(tag?);
    }
    Ok(results)
}

pub fn tag_list(app: &AppHandle) -> Result<Vec<Tag>, Box<dyn std::error::Error>> {
    tag_list_path(&db_path(app))
}

pub fn tag_create(
    app: &AppHandle,
    name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let name_lower = name.to_lowercase();
    // INSERT OR IGNORE — if tag already exists (e.g. AI returns duplicate),
    // silently skip and return the existing id
    let id = Ulid::new().to_string();
    let affected = conn.execute(
        "INSERT OR IGNORE INTO tags (id, name) VALUES (?1, ?2)",
        params![&id, &name_lower],
    )?;
    if affected > 0 {
        return Ok(id);
    }
    // Tag already exists — look up its id
    let existing: String = conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        params![&name_lower],
        |r| r.get(0),
    )?;
    Ok(existing)
}

pub fn tag_delete(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn tag_rename(
    app: &AppHandle,
    id: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
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
    media_tags_get_with_variant(app, media_id, None)
}

pub fn media_tags_get_with_variant(
    app: &AppHandle,
    media_id: &str,
    variant_id: Option<&str>,
) -> Result<Vec<Tag>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut results = Vec::new();
    if let Some(vid) = variant_id {
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, mt.source, mt.confidence FROM tags t
             JOIN media_tags mt ON t.id = mt.tag_id
             WHERE mt.media_id = ?1 AND mt.variant_id = ?2
             ORDER BY t.name",
        )?;
        let tag_iter = stmt.query_map(params![media_id, vid], |row| {
            Ok(Tag { id: row.get(0)?, name: row.get(1)?, source: row.get(2)?, confidence: row.get(3)?, item_count: None })
        })?;
        for tag in tag_iter { results.push(tag?); }
    } else {
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, mt.source, mt.confidence FROM tags t
             JOIN media_tags mt ON t.id = mt.tag_id
             WHERE mt.media_id = ?1 AND mt.variant_id IS NULL
             ORDER BY t.name",
        )?;
        let tag_iter = stmt.query_map(params![media_id], |row| {
            Ok(Tag { id: row.get(0)?, name: row.get(1)?, source: row.get(2)?, confidence: row.get(3)?, item_count: None })
        })?;
        for tag in tag_iter { results.push(tag?); }
    }
    Ok(results)
}

pub fn media_tag_add(
    app: &AppHandle,
    media_id: &str,
    tag_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    media_tag_add_with_source(app, media_id, tag_id, None, None)
}

pub fn media_tag_add_with_source(
    app: &AppHandle,
    media_id: &str,
    tag_id: &str,
    confidence: Option<f64>,
    source: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    media_tag_add_internal(app, media_id, None, tag_id, confidence, source)
}

pub fn media_tag_add_for_variant(
    app: &AppHandle,
    media_id: &str,
    variant_id: &str,
    tag_id: &str,
    source: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    media_tag_add_internal(app, media_id, Some(variant_id), tag_id, None, source)
}

fn media_tag_add_internal(
    app: &AppHandle,
    media_id: &str,
    variant_id: Option<&str>,
    tag_id: &str,
    confidence: Option<f64>,
    source: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "INSERT OR REPLACE INTO media_tags (media_id, variant_id, tag_id, confidence, source) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![media_id, variant_id, tag_id, confidence, source],
    )?;
    let mid = media_id.to_string();
    drop(conn);
    let _ = fts_sync(app, &mid);
    Ok(())
}

pub fn media_tag_add_batch(
    app: &AppHandle,
    media_ids: &[String],
    tag_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute("BEGIN TRANSACTION", [])?;
    let result = (|| {
        for media_id in media_ids {
            conn.execute(
                "INSERT OR IGNORE INTO media_tags (media_id, tag_id, confidence, source) VALUES (?1, ?2, NULL, NULL)",
                params![media_id, tag_id],
            )?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            conn.execute("COMMIT", [])?;
            drop(conn);
            for mid in media_ids {
                let _ = fts_sync(app, mid);
            }
            Ok(())
        }
        Err(e) => { let _ = conn.execute("ROLLBACK", []); Err(e) }
    }
}

pub fn media_tag_remove(
    app: &AppHandle,
    media_id: &str,
    tag_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    media_tag_remove_with_variant(app, media_id, None, tag_id)
}

pub fn media_tag_remove_with_variant(
    app: &AppHandle,
    media_id: &str,
    variant_id: Option<&str>,
    tag_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    if let Some(vid) = variant_id {
        conn.execute(
            "DELETE FROM media_tags WHERE media_id = ?1 AND variant_id = ?2 AND tag_id = ?3",
            params![media_id, vid, tag_id],
        )?;
    } else {
        conn.execute(
            "DELETE FROM media_tags WHERE media_id = ?1 AND variant_id IS NULL AND tag_id = ?2",
            params![media_id, tag_id],
        )?;
    }
    let mid = media_id.to_string();
    drop(conn);
    let _ = fts_sync(app, &mid);
    Ok(())
}

pub fn media_tags_clear(
    app: &AppHandle,
    media_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "DELETE FROM media_tags WHERE media_id = ?1",
        params![media_id],
    )?;
    let mid = media_id.to_string();
    drop(conn);
    let _ = fts_sync(app, &mid);
    Ok(())
}

pub fn media_tags_intersect(
    app: &AppHandle,
    media_ids: &[String],
) -> Result<Vec<Tag>, Box<dyn std::error::Error>> {
    if media_ids.is_empty() {
        return Ok(Vec::new());
    }
    let conn = get_conn(app)?;
    let placeholders: Vec<String> = media_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT t.id, t.name, mt.source, mt.confidence
         FROM tags t
         JOIN media_tags mt ON mt.tag_id = t.id
         WHERE mt.media_id IN ({})
         GROUP BY t.id
         HAVING COUNT(DISTINCT mt.media_id) = {}",
        placeholders.join(", "),
        media_ids.len()
    );
    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::types::ToSql> = media_ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            source: row.get(2)?,
            confidence: row.get(3)?,
            item_count: None,
        })
    })?;
    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

#[derive(Debug, Clone, Copy)]
pub enum TagSearchMode {
    Intersection,
    Union,
}

pub fn media_search_by_tags_path(
    db_path: &Path,
    tag_names: &[String],
    sort_by: &str,
    descending: bool,
    mode: TagSearchMode,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    if tag_names.is_empty() {
        return list_media_path(db_path, sort_by, descending, 0, u32::MAX);
    }

    let conn = Connection::open(db_path)?;

    let order = if descending { "DESC" } else { "ASC" };
    let sort_column = match sort_by {
        "created_at" => "created_at",
        "modified_at" => "modified_at",
        "file_size" => "file_size",
        "width" => "width",
        "height" => "height",
        _ => "imported_at",
    };

    let placeholders = tag_names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = match mode {
        TagSearchMode::Intersection => format!(
            "SELECT m.id, m.source_path, m.width, m.height, m.file_size, m.created_at, m.modified_at, m.imported_at, m.source_url, m.page_url, m.source, m.sha256, m.deleted_at, m.display_variant_id, m.lqip
             FROM media m
             JOIN media_tags mt ON m.id = mt.media_id
             JOIN tags t ON mt.tag_id = t.id
             WHERE m.deleted_at IS NULL AND t.name IN ({})
             GROUP BY m.id
             HAVING COUNT(DISTINCT t.id) = {}
             ORDER BY m.{} {}",
            placeholders, tag_names.len(), sort_column, order
        ),
        TagSearchMode::Union => format!(
            "SELECT DISTINCT m.id, m.source_path, m.width, m.height, m.file_size, m.created_at, m.modified_at, m.imported_at, m.source_url, m.page_url, m.source, m.sha256, m.deleted_at, m.display_variant_id, m.lqip
             FROM media m
             JOIN media_tags mt ON m.id = mt.media_id
             JOIN tags t ON mt.tag_id = t.id
             WHERE m.deleted_at IS NULL AND t.name IN ({})
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
            phash: None,
            width: row.get(2)?,
            height: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
            modified_at: row.get(6)?,
            imported_at: row.get(7)?,
            source_url: row.get(8)?,
            page_url: row.get(9)?,
            source: row.get(10)?,
            sha256: row.get(11)?,
            deleted_at: row.get(12)?,
            display_variant_id: row.get(13)?,
            lqip: row.get(14)?,
            thumb_256: None,
        })
    })?;

    let mut results = Vec::new();
    for media in media_iter {
        results.push(media?);
    }

    Ok(results)
}

// --- FTS5 full-text search ---

/// Rebuild the FTS index for a single media_id from all its captions and tags.
pub fn fts_sync(app: &AppHandle, media_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;

    // Gather all captions and tags for this media
    let mut captions = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT text FROM captions WHERE media_id = ?1",
        )?;
        for row in stmt.query_map(params![media_id], |r| r.get::<_, String>(0))? {
            captions.push(row?);
        }
    }
    let mut tags = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT t.name FROM tags t JOIN media_tags mt ON t.id = mt.tag_id WHERE mt.media_id = ?1",
        )?;
        for row in stmt.query_map(params![media_id], |r| r.get::<_, String>(0))? {
            tags.push(row?);
        }
    }

    let search_text = if captions.is_empty() && tags.is_empty() {
        String::new()
    } else {
        let mut parts = captions;
        parts.extend(tags);
        parts.join(" ")
    };

    // Delete existing entry and insert new
    conn.execute("DELETE FROM media_fts WHERE media_id = ?1", params![media_id])?;
    if !search_text.is_empty() {
        conn.execute(
            "INSERT INTO media_fts (media_id, search_text) VALUES (?1, ?2)",
            params![media_id, search_text],
        )?;
    }
    Ok(())
}

/// Rebuild FTS index for all media — only if empty (first run after migration).
pub fn fts_rebuild_all(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let count: i64 = {
        let conn = get_conn(app)?;
        conn.query_row("SELECT COUNT(*) FROM media_fts", [], |r| r.get(0))?
    };
    if count > 0 {
        return Ok(());
    }

    let conn = get_conn(app)?;
    let mut stmt = conn.prepare("SELECT id FROM media WHERE deleted_at IS NULL")?;
    let ids: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    drop(conn);

    eprintln!("[fts] rebuilding index for {} media...", ids.len());
    for id in &ids {
        fts_sync(app, id)?;
    }
    eprintln!("[fts] rebuild complete");
    Ok(())
}

/// Search media via FTS5. Returns media_ids ranked by BM25, up to `limit` results.
pub fn fts_search(
    app: &AppHandle,
    query: &str,
    limit: u32,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;

    // Simple query — escape FTS5 special chars
    let cleaned: String = query
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    if cleaned.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Use double-quoted terms for better matching
    let fts_query = cleaned
        .split_whitespace()
        .map(|w| format!("\"{}\"", w))
        .collect::<Vec<_>>()
        .join(" OR ");

    let mut stmt = conn.prepare(
        "SELECT media_id FROM media_fts WHERE media_fts MATCH ?1 ORDER BY rank LIMIT ?2",
    )?;
    let results = stmt
        .query_map(params![fts_query, limit as i64], |r| r.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(results)
}

pub fn media_search_by_tags(
    app: &AppHandle,
    tag_names: &[String],
    sort_by: &str,
    descending: bool,
    mode: TagSearchMode,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    if tag_names.is_empty() {
        return list_media(app, sort_by, descending, 0, u32::MAX);
    }
    let path = db_path(app);
    let mut results = media_search_by_tags_path(&path, tag_names, sort_by, descending, mode)?;
    resolve_thumb_paths(app, &mut results);
    Ok(results)
}

pub fn media_query_filtered_path(
    db_path: &Path,
    media_ids: Option<&[String]>,
    dimensions: &[crate::search::parser::DimFilter],
    date_range: &Option<crate::search::parser::DateRange>,
    file_size: &Option<crate::search::parser::SizeFilter>,
    sort_by: &str,
    descending: bool,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    use crate::search::parser::{Comparison, DimFilter, SizeOp};

    let conn = Connection::open(db_path)?;

    let order = if descending { "DESC" } else { "ASC" };
    let sort_column = match sort_by {
        "created_at" => "m.created_at",
        "modified_at" => "m.modified_at",
        "file_size" => "m.file_size",
        "width" => "m.width",
        "height" => "m.height",
        _ => "m.imported_at",
    };

    let mut conditions: Vec<String> = vec!["m.deleted_at IS NULL".to_string()];
    let mut bind_values: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(ids) = media_ids {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let ph: Vec<String> = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", bind_values.len() + i + 1))
            .collect();
        conditions.push(format!("m.id IN ({})", ph.join(",")));
        for id in ids {
            bind_values.push(rusqlite::types::Value::Text(id.clone()));
        }
    }

    for dim in dimensions {
        let (col, op) = match dim {
            DimFilter::Width { op } => ("m.width", op),
            DimFilter::Height { op } => ("m.height", op),
        };
        match op {
            Comparison::Gt(v) => {
                bind_values.push(rusqlite::types::Value::Integer(*v));
                conditions.push(format!("{} > ?{}", col, bind_values.len()));
            }
            Comparison::Lt(v) => {
                bind_values.push(rusqlite::types::Value::Integer(*v));
                conditions.push(format!("{} < ?{}", col, bind_values.len()));
            }
            Comparison::Range(lo, hi) => {
                bind_values.push(rusqlite::types::Value::Integer(*lo));
                bind_values.push(rusqlite::types::Value::Integer(*hi));
                let n = bind_values.len();
                conditions.push(format!(
                    "{} BETWEEN ?{} AND ?{}",
                    col,
                    n - 1,
                    n
                ));
            }
        }
    }

    if let Some(dr) = date_range {
        bind_values.push(rusqlite::types::Value::Text(dr.start.clone()));
        bind_values.push(rusqlite::types::Value::Text(dr.end.clone()));
        let n = bind_values.len();
        conditions.push(format!(
            "m.created_at BETWEEN ?{} AND ?{}",
            n - 1,
            n
        ));
    }

    if let Some(sf) = file_size {
        match &sf.op {
            SizeOp::GreaterThan(v) => {
                bind_values.push(rusqlite::types::Value::Integer(*v as i64));
                conditions.push(format!("m.file_size > ?{}", bind_values.len()));
            }
            SizeOp::LessThan(v) => {
                bind_values.push(rusqlite::types::Value::Integer(*v as i64));
                conditions.push(format!("m.file_size < ?{}", bind_values.len()));
            }
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT m.id, m.source_path, m.width, m.height, m.file_size,
                m.created_at, m.modified_at, m.imported_at,
                m.source_url, m.page_url, m.source, m.sha256, m.deleted_at,
                m.display_variant_id, m.lqip
         FROM media m {} ORDER BY {} {}",
        where_clause, sort_column, order
    );

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        bind_values.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();

    let iter = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(Media {
            id: row.get(0)?,
            source_path: row.get(1)?,
            phash: None,
            width: row.get(2)?,
            height: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
            modified_at: row.get(6)?,
            imported_at: row.get(7)?,
            source_url: row.get(8)?,
            page_url: row.get(9)?,
            source: row.get(10)?,
            sha256: row.get(11)?,
            deleted_at: row.get(12)?,
            display_variant_id: row.get(13)?,
            lqip: row.get(14)?,
            thumb_256: None,
        })
    })?;

    let mut results = Vec::new();
    for r in iter {
        results.push(r?);
    }
    Ok(results)
}

pub fn media_query_filtered(
    app: &AppHandle,
    media_ids: Option<&[String]>,
    dimensions: &[crate::search::parser::DimFilter],
    date_range: &Option<crate::search::parser::DateRange>,
    file_size: &Option<crate::search::parser::SizeFilter>,
    sort_by: &str,
    descending: bool,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    media_query_filtered_path(&db_path(app), media_ids, dimensions, date_range, file_size, sort_by, descending)
}

// --- Variant operations ---

pub fn variant_list(
    app: &AppHandle,
    media_id: &str,
) -> Result<Vec<Variant>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT id, media_id, preset_name, format, width, height, quality, file_size, file_path, label, source
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
            label: row.get(9)?,
            source: row.get(10)?,
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
    let conn = get_conn(app)?;
    conn.execute(
        "INSERT OR REPLACE INTO variants (id, media_id, preset_name, format, width, height, quality, file_size, file_path, label, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
            &variant.label,
            &variant.source,
        ],
    )?;
    Ok(())
}

pub fn variant_delete(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute("DELETE FROM variants WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn variant_get_by_id(
    app: &AppHandle,
    id: &str,
) -> Result<Option<Variant>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT id, media_id, preset_name, format, width, height, quality, file_size, file_path, label, source
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
            label: row.get(9)?,
            source: row.get(10)?,
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
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT id, media_id, preset_name, format, width, height, quality, file_size, file_path, label, source
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
            label: row.get(9)?,
            source: row.get(10)?,
        })
    })?;
    if let Some(row) = rows.next() {
        return Ok(Some(row?));
    }
    Ok(None)
}

// --- Caption operations ---

pub fn caption_list(
    app: &AppHandle,
    media_id: &str,
) -> Result<Vec<Caption>, Box<dyn std::error::Error>> {
    caption_list_path(&db_path(app), media_id)
}

pub fn caption_list_path(
    db_path: &Path,
    media_id: &str,
) -> Result<Vec<Caption>, Box<dyn std::error::Error>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, media_id, variant_id, text, source, created_at, updated_at
         FROM captions WHERE media_id = ?1 ORDER BY created_at",
    )?;
    let caption_iter = stmt.query_map(params![media_id], |row| {
        Ok(Caption {
            id: row.get(0)?,
            media_id: row.get(1)?,
            variant_id: row.get(2)?,
            text: row.get(3)?,
            source: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })?;
    let mut results = Vec::new();
    for c in caption_iter {
        results.push(c?);
    }
    Ok(results)
}

pub fn caption_create(
    app: &AppHandle,
    media_id: &str,
    text: &str,
) -> Result<Caption, Box<dyn std::error::Error>> {
    caption_create_with_source(app, media_id, text, None)
}

pub fn caption_create_for_variant(
    app: &AppHandle,
    media_id: &str,
    variant_id: &str,
    text: &str,
    source: Option<&str>,
) -> Result<Caption, Box<dyn std::error::Error>> {
    caption_create_internal(app, media_id, text, source, Some(variant_id))
}

pub fn caption_create_with_source(
    app: &AppHandle,
    media_id: &str,
    text: &str,
    source: Option<&str>,
) -> Result<Caption, Box<dyn std::error::Error>> {
    caption_create_internal(app, media_id, text, source, None)
}

fn caption_create_internal(
    app: &AppHandle,
    media_id: &str,
    text: &str,
    source: Option<&str>,
    variant_id: Option<&str>,
) -> Result<Caption, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let id = Ulid::new().to_string();
    conn.execute(
        "INSERT INTO captions (id, media_id, variant_id, text, source) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, media_id, variant_id, text, source],
    )?;
    let mid = media_id.to_string();
    drop(conn);
    let _ = fts_sync(app, &mid);
    Ok(Caption {
        id,
        media_id: media_id.to_string(),
        variant_id: variant_id.map(|s| s.to_string()),
        text: text.to_string(),
        source: source.map(|s| s.to_string()),
        created_at: None,
        updated_at: None,
    })
}

pub fn caption_update(
    app: &AppHandle,
    id: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    // Look up media_id before updating
    let mid: String = conn.query_row(
        "SELECT media_id FROM captions WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE captions SET text = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        params![text, id],
    )?;
    drop(conn);
    let _ = fts_sync(app, &mid);
    Ok(())
}

pub fn caption_delete(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mid: String = conn.query_row(
        "SELECT media_id FROM captions WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    conn.execute("DELETE FROM captions WHERE id = ?1", params![id])?;
    drop(conn);
    let _ = fts_sync(app, &mid);
    Ok(())
}

// --- Embedding operations ---

pub fn embedding_insert(
    app: &AppHandle,
    media_id: &str,
    model: &str,
    content_type: &str,
    vector: &[f32],
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let bytes: Vec<u8> = vector.iter().flat_map(|v| v.to_le_bytes()).collect();
    conn.execute(
        "INSERT OR REPLACE INTO embeddings (media_id, model, content_type, vector)
         VALUES (?1, ?2, ?3, ?4)",
        params![media_id, model, content_type, bytes],
    )?;
    Ok(())
}

pub fn embedding_get(
    app: &AppHandle,
    media_id: &str,
    model: &str,
    content_type: &str,
) -> Result<Option<Vec<f32>>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT vector FROM embeddings WHERE media_id = ?1 AND model = ?2 AND content_type = ?3",
    )?;
    let mut rows = stmt.query_map(params![media_id, model, content_type], |row| {
        let bytes: Vec<u8> = row.get(0)?;
        let mut vec = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(chunk);
            vec.push(f32::from_le_bytes(arr));
        }
        Ok(vec)
    })?;
    if let Some(row) = rows.next() {
        return Ok(Some(row?));
    }
    Ok(None)
}

pub fn embedding_delete_for_media(
    app: &AppHandle,
    media_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "DELETE FROM embeddings WHERE media_id = ?1",
        params![media_id],
    )?;
    Ok(())
}

pub fn embedding_info_list(
    app: &AppHandle,
    media_id: &str,
) -> Result<Vec<EmbeddingInfo>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT model, content_type, length(vector) / 4 as vec_len, created_at
         FROM embeddings WHERE media_id = ?1 ORDER BY content_type",
    )?;
    let iter = stmt.query_map(params![media_id], |row| {
        Ok(EmbeddingInfo {
            model: row.get(0)?,
            content_type: row.get(1)?,
            vec_dim: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;
    let mut results = Vec::new();
    for r in iter {
        results.push(r?);
    }
    Ok(results)
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingInfo {
    pub model: String,
    pub content_type: String,
    pub vec_dim: usize,
    pub created_at: Option<String>,
}

pub fn embedding_get_all_by_model(
    app: &AppHandle,
    model: &str,
) -> Result<Vec<(String, String, Vec<f32>)>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT media_id, content_type, vector FROM embeddings WHERE model = ?1 AND content_type = 'caption'",
    )?;
    let iter = stmt.query_map(params![model], |row| {
        let media_id: String = row.get(0)?;
        let content_type: String = row.get(1)?;
        let bytes: Vec<u8> = row.get(2)?;
        let mut vec = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(chunk);
            vec.push(f32::from_le_bytes(arr));
        }
        Ok((media_id, content_type, vec))
    })?;
    let mut results = Vec::new();
    for r in iter {
        results.push(r?);
    }
    Ok(results)
}

// --- Saved filters ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedFilter {
    pub name: String,
    pub query: String,
}

pub fn saved_filters_get_all(app: &AppHandle) -> Result<Vec<SavedFilter>, Box<dyn std::error::Error>> {
    let json = setting_get(app, "saved_filters")?
        .unwrap_or_else(|| "[]".to_string());
    Ok(serde_json::from_str(&json)?)
}

pub fn saved_filters_save(app: &AppHandle, filter: &SavedFilter) -> Result<(), Box<dyn std::error::Error>> {
    let mut filters = saved_filters_get_all(app)?;
    if let Some(pos) = filters.iter().position(|f| f.name == filter.name) {
        filters[pos] = filter.clone();
    } else {
        filters.push(filter.clone());
    }
    setting_set(app, "saved_filters", &serde_json::to_string(&filters)?)
}

pub fn saved_filters_delete(app: &AppHandle, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let filters = saved_filters_get_all(app)?;
    let filters: Vec<SavedFilter> = filters
        .into_iter()
        .filter(|f| f.name != name)
        .collect();
    setting_set(app, "saved_filters", &serde_json::to_string(&filters)?)
}

// --- Settings operations ---

pub fn setting_get(app: &AppHandle, key: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query_map(params![key], |row| Ok(row.get::<_, String>(0)?))?;
    if let Some(row) = rows.next() {
        return Ok(Some(row?));
    }
    Ok(None)
}

pub fn setting_set(
    app: &AppHandle,
    key: &str,
    value: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

// --- Soft delete / trash ---

pub fn media_soft_delete(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "UPDATE media SET deleted_at = ?1 WHERE id = ?2",
        params![chrono::Utc::now().to_rfc3339(), id],
    )?;
    Ok(())
}

pub fn media_recover(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "UPDATE media SET deleted_at = NULL WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn media_get_display_variant(
    app: &AppHandle,
    media_id: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let display_id: Option<String> = conn
        .query_row(
            "SELECT display_variant_id FROM media WHERE id = ?1",
            params![media_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    if let Some(vid) = display_id {
        let file_path: Option<String> = conn
            .query_row(
                "SELECT file_path FROM variants WHERE id = ?1",
                params![vid],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        return Ok(file_path);
    }
    Ok(None)
}

/// Batch version of media_get_display_variant — single query for N media IDs.
pub fn media_get_display_variants_batch(
    app: &AppHandle,
    media_ids: &[String],
) -> Result<std::collections::HashMap<String, String>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;

    if media_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let placeholders: Vec<String> = media_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT m.id, v.file_path FROM media m LEFT JOIN variants v ON m.display_variant_id = v.id WHERE m.id IN ({})",
        placeholders.join(", ")
    );

    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::types::ToSql> = media_ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
    let rows = stmt.query_map(params.as_slice(), |row| {
        let media_id: String = row.get(0)?;
        let file_path: Option<String> = row.get(1)?;
        Ok((media_id, file_path))
    })?;

    let mut map = std::collections::HashMap::new();
    for row in rows {
        if let Some((id, file_path)) = row.ok() {
            if let Some(fp) = file_path {
                map.insert(id, fp);
            }
        }
    }
    Ok(map)
}

pub fn media_set_display_variant(
    app: &AppHandle,
    media_id: &str,
    variant_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "UPDATE media SET display_variant_id = ?1 WHERE id = ?2",
        params![variant_id, media_id],
    )?;
    Ok(())
}

pub fn media_permanent_delete(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let app_dir = app.path().app_data_dir().expect("app data dir");

    // Delete files
    let library_dir = app_dir.join("library");
    let thumbs_dir = app_dir.join("thumbnails");
    let variants_dir = app_dir.join("variants");

    // Find and delete library file
    if let Ok(entries) = std::fs::read_dir(&library_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(id) {
                let _ = std::fs::remove_file(entry.path());
                break;
            }
        }
    }

    // Delete thumbnails
    for suffix in &["256", "512"] {
        let thumb = thumbs_dir.join(format!("{}_{}.jpg", id, suffix));
        let _ = std::fs::remove_file(&thumb);
    }

    // Delete variants
    if let Ok(entries) = std::fs::read_dir(&variants_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(id) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    // Delete DB record (cascades to tags/captions/embeddings/variants)
    let conn = get_conn(app)?;
    conn.execute("DELETE FROM media WHERE id = ?1", params![id])?;

    Ok(())
}

pub fn media_list_trash(
    app: &AppHandle,
    sort_by: &str,
    descending: bool,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;

    let order = if descending { "DESC" } else { "ASC" };
    let sort_column = match sort_by {
        "created_at" => "created_at",
        "modified_at" => "modified_at",
        "deleted_at" => "deleted_at",
        "file_size" => "file_size",
        _ => "deleted_at",
    };

    let sql = format!(
        "SELECT id, source_path, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, deleted_at, display_variant_id, lqip
         FROM media
         WHERE deleted_at IS NOT NULL
         ORDER BY {} {}",
        sort_column, order
    );

    let mut stmt = conn.prepare(&sql)?;
    let media_iter = stmt.query_map([], |row| {
        Ok(Media {
            id: row.get(0)?,
            source_path: row.get(1)?,
            phash: None,
            width: row.get(2)?,
            height: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
            modified_at: row.get(6)?,
            imported_at: row.get(7)?,
            source_url: row.get(8)?,
            page_url: row.get(9)?,
            source: row.get(10)?,
            sha256: row.get(11)?,
            deleted_at: row.get(12)?,
            display_variant_id: row.get(13)?,
            lqip: row.get(14)?,
            thumb_256: None,
        })
    })?;

    let mut results = Vec::new();
    for media in media_iter {
        results.push(media?);
    }

    resolve_thumb_paths(app, &mut results);
    Ok(results)
}

pub fn media_empty_trash(app: &AppHandle) -> Result<usize, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare("SELECT id FROM media WHERE deleted_at IS NOT NULL")?;
    let ids: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    let count = ids.len();
    for id in &ids {
        media_permanent_delete(app, id)?;
    }

    Ok(count)
}

pub fn media_find_similar(
    app: &AppHandle,
    threshold: u32,
) -> Result<Vec<Vec<Media>>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT id, phash, width, height, file_size
         FROM media WHERE phash IS NOT NULL AND deleted_at IS NULL",
    )?;

    struct Item {
        id: String,
        hash: u64,
        width: i32,
        height: i32,
        file_size: i64,
    }

    let items: Vec<Item> = stmt
        .query_map([], |row| {
            let phash_bytes: Option<Vec<u8>> = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i64>(5)?,
                phash_bytes,
            ))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(id, width, height, file_size, phash_bytes)| {
            phash_bytes.and_then(|bytes| {
                let arr: [u8; 8] = bytes.try_into().ok()?;
                Some(Item {
                    id,
                    hash: u64::from_le_bytes(arr),
                    width,
                    height,
                    file_size,
                })
            })
        })
        .collect();

    drop(stmt);
    drop(conn);

    // Pre-filter: group by file_size bucket to avoid comparing completely different images
    // Bucket = floor(log2(file_size)), so 100KB and 200KB are in same bucket, 100KB and 10MB are not
    use std::collections::HashMap;
    let mut buckets: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let bucket = if item.file_size > 0 {
            (item.file_size as f64).log2().floor() as u32
        } else {
            0
        };
        buckets.entry(bucket).or_default().push(i);
    }

    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut used: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Compare within each bucket + adjacent buckets (±1)
    for (&bucket, indices) in &buckets {
        // Collect candidates: current bucket + adjacent
        let mut candidates: Vec<usize> = indices.clone();
        for adj in &[bucket.wrapping_sub(1), bucket + 1] {
            if let Some(extra) = buckets.get(adj) {
                candidates.extend(extra);
            }
        }

        for &i in indices {
            if used.contains(&i) {
                continue;
            }
            let mut group = vec![items[i].id.clone()];
            used.insert(i);

            for &j in &candidates {
                if used.contains(&j) || j <= i {
                    continue;
                }
                // Pre-filter: skip if aspect ratios differ by > 2x (e.g., landscape vs portrait)
                let ar_i = items[i].width as f64 / items[i].height.max(1) as f64;
                let ar_j = items[j].width as f64 / items[j].height.max(1) as f64;
                let ar_ratio = if ar_i > ar_j { ar_i / ar_j } else { ar_j / ar_i };
                if ar_ratio > 2.0 {
                    continue;
                }
                // Pre-filter: skip if file sizes differ by > 4x
                let fs_i = items[i].file_size.max(1) as f64;
                let fs_j = items[j].file_size.max(1) as f64;
                let fs_ratio = if fs_i > fs_j { fs_i / fs_j } else { fs_j / fs_i };
                if fs_ratio > 4.0 {
                    continue;
                }

                let dist = crate::media::phash::hamming_distance(items[i].hash, items[j].hash);
                if dist <= threshold {
                    group.push(items[j].id.clone());
                    used.insert(j);
                }
            }

            if group.len() > 1 {
                groups.push(group);
            }
        }
    }

    // Resolve groups to Media objects
    let mut result = Vec::new();
    for group in &groups {
        let mut media_list = Vec::new();
        for id in group {
            if let Ok(Some(media)) = media_get_by_id(app, id) {
                media_list.push(media);
            }
        }
        if media_list.len() > 1 {
            result.push(media_list);
        }
    }

    Ok(result)
}

fn media_get_by_id(app: &AppHandle, id: &str) -> Result<Option<Media>, Box<dyn std::error::Error>> {
    let list = media_get_batch(app, &[id.to_string()])?;
    Ok(list.into_iter().next())
}
