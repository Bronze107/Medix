use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use ulid::Ulid;

use crate::captions::Caption;
use crate::media::{BrowseItem, BrowseVisibility, Media};
use crate::tag::Tag;

pub mod comfyui;

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
    let app_dir = app
        .path()
        .app_data_dir()
        .expect("Failed to get app data dir");
    fs::create_dir_all(&app_dir).expect("Failed to create app data dir");
    app_dir.join("medix.db")
}

/// Get a pooled connection from the Tauri managed state.
pub(crate) fn get_conn(
    app: &AppHandle,
) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, String> {
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

/// Create a fresh test database at the given path with all migrations applied.
/// The parent directory must exist.
pub fn setup_test_db(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    run_migrations(&mut conn)?;
    Ok(())
}

pub fn run_migrations(conn: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    // The ledger is created up front because we must consult it before deciding
    // whether the legacy `variants` table should exist at all.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );",
    )?;

    // Once 0028_drop_variants is recorded, the `variants` table is intentionally
    // gone and must never be re-created. Migrations 0003/0011/0013/0014/0019 all
    // reference it; without this flag they resurrect the table and leave dangling
    // foreign keys behind (see 0033_repair_dangling_variant_fks).
    let variants_dropped: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0028_drop_variants'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    conn.execute_batch(
        "
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

    // 0003_variants DDL — only on databases where 0028_drop_variants has not run.
    // Applying it unconditionally re-created the table on every startup, which let
    // 0011/0013/0014 re-attach foreign keys to it right before 0028 dropped it again.
    if !variants_dropped {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS variants (
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

            CREATE INDEX IF NOT EXISTS idx_variants_media ON variants(media_id);",
        )?;
    }

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

    if !has_label && !variants_dropped {
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
    let has_display_variant: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('media') WHERE name='display_variant_id'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !has_display_variant && !variants_dropped {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0013_variant_annotation');
             ALTER TABLE captions ADD COLUMN variant_id TEXT REFERENCES variants(id) ON DELETE CASCADE;
             ALTER TABLE embeddings ADD COLUMN variant_id TEXT REFERENCES variants(id) ON DELETE CASCADE;
             ALTER TABLE media ADD COLUMN display_variant_id TEXT REFERENCES variants(id) ON DELETE SET NULL;
             CREATE INDEX IF NOT EXISTS idx_captions_variant ON captions(variant_id);
             CREATE INDEX IF NOT EXISTS idx_embeddings_variant ON embeddings(variant_id);",
        )?;
    } else {
        // Ensure the migration entry exists so subsequent passes don't re-try
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0013_variant_annotation');",
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
    if !has_variant_tags && !variants_dropped {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0014_variant_tags');
             ALTER TABLE media_tags ADD COLUMN variant_id TEXT REFERENCES variants(id) ON DELETE CASCADE;
             CREATE INDEX IF NOT EXISTS idx_media_tags_variant ON media_tags(variant_id);",
        )?;
    } else {
        // Ensure the migration entry exists so subsequent passes don't re-try.
        // Without this else branch, 0029-0032 deleting media_tags.variant_id made
        // this block re-add the column (and its FK to the dropped variants table)
        // on the very next startup.
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0014_variant_tags');",
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

    // --- 0018_video_support ---
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0018_video_support'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            let columns: Vec<String> = {
                let mut stmt = conn.prepare("PRAGMA table_info('media')")?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
                rows.filter_map(|r| r.ok()).collect()
            };
            let mut sql = String::from(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0018_video_support');",
            );
            if !columns.contains(&"media_type".to_string()) {
                sql.push_str("ALTER TABLE media ADD COLUMN media_type TEXT DEFAULT 'image';");
            }
            if !columns.contains(&"duration".to_string()) {
                sql.push_str("ALTER TABLE media ADD COLUMN duration REAL;");
            }
            if !columns.contains(&"video_codec".to_string()) {
                sql.push_str("ALTER TABLE media ADD COLUMN video_codec TEXT;");
            }
            if !columns.contains(&"video_fps".to_string()) {
                sql.push_str("ALTER TABLE media ADD COLUMN video_fps REAL;");
            }
            sql.push_str("CREATE INDEX IF NOT EXISTS idx_media_type ON media(media_type);");
            conn.execute_batch(&sql)?;
        }
    }

    // --- 0019_video_variants ---
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0019_video_variants'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            if variants_dropped {
                // `variants` is gone — record the migration instead of ALTERing a
                // missing table.
                conn.execute_batch(
                    "INSERT OR IGNORE INTO _migrations (name) VALUES ('0019_video_variants');",
                )?;
            } else {
                let columns: Vec<String> = {
                    let mut stmt = conn.prepare("PRAGMA table_info('variants')")?;
                    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
                    rows.filter_map(|r| r.ok()).collect()
                };
                let mut sql = String::from(
                    "INSERT OR IGNORE INTO _migrations (name) VALUES ('0019_video_variants');",
                );
                if !columns.contains(&"media_type".to_string()) {
                    sql.push_str(
                        "ALTER TABLE variants ADD COLUMN media_type TEXT DEFAULT 'image';",
                    );
                }
                if !columns.contains(&"duration".to_string()) {
                    sql.push_str("ALTER TABLE variants ADD COLUMN duration REAL;");
                }
                if !columns.contains(&"video_codec".to_string()) {
                    sql.push_str("ALTER TABLE variants ADD COLUMN video_codec TEXT;");
                }
                if !columns.contains(&"video_fps".to_string()) {
                    sql.push_str("ALTER TABLE variants ADD COLUMN video_fps REAL;");
                }
                conn.execute_batch(&sql)?;
            }
        }
    }

    // --- 0020_browse_indexes ---
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0020_browse_indexes'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0020_browse_indexes');
                 CREATE INDEX IF NOT EXISTS idx_media_display_variant ON media(display_variant_id);
                 CREATE INDEX IF NOT EXISTS idx_media_deleted_imported ON media(deleted_at, imported_at);",
            )?;
        }
    }

    // --- 0021_embeddings_unique ---
    // Rebuild embeddings table with proper unique constraints so original and variant
    // embeddings can coexist without overwriting each other.
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0021_embeddings_unique'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0021_embeddings_unique');
                 CREATE TABLE embeddings_new (
                     media_id TEXT NOT NULL,
                     model TEXT NOT NULL,
                     content_type TEXT NOT NULL,
                     variant_id TEXT,
                     vector BLOB NOT NULL,
                     created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                     FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
                     FOREIGN KEY (variant_id) REFERENCES variants(id) ON DELETE CASCADE
                 );
                 INSERT INTO embeddings_new SELECT media_id, model, content_type, variant_id, vector, created_at FROM embeddings;
                 DROP TABLE embeddings;
                 ALTER TABLE embeddings_new RENAME TO embeddings;
                 CREATE UNIQUE INDEX idx_embeddings_unique_orig ON embeddings(media_id, model, content_type) WHERE variant_id IS NULL;
                 CREATE UNIQUE INDEX idx_embeddings_unique_var ON embeddings(media_id, model, content_type, variant_id) WHERE variant_id IS NOT NULL;
                 CREATE INDEX idx_embeddings_media ON embeddings(media_id);
                 CREATE INDEX idx_embeddings_model ON embeddings(model);
                 CREATE INDEX idx_embeddings_variant ON embeddings(variant_id);",
            )?;
        }
    }

    // --- 0022_variant_presets ---
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0022_variant_presets'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0022_variant_presets');
                 CREATE TABLE IF NOT EXISTS variant_presets (
                     name TEXT PRIMARY KEY,
                     label TEXT NOT NULL,
                     format TEXT NOT NULL,
                     max_width INTEGER,
                     max_height INTEGER,
                     quality INTEGER NOT NULL,
                     created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                 );",
            )?;
        }
    }

    // --- 0023_variant_presets_resize_filter ---
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0023_variant_presets_resize_filter'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0023_variant_presets_resize_filter');
                 ALTER TABLE variant_presets ADD COLUMN resize_filter TEXT DEFAULT 'triangle';",
            )?;
        }
    }

    // 0024_comfyui_workflows
    {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0024_comfyui_workflows');
             CREATE TABLE IF NOT EXISTS comfyui_workflows (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 workflow_type TEXT NOT NULL DEFAULT 'generate',
                 workflow_json TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now')),
                 updated_at TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )?;
    }

    // 0025_media_lineage
    {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0025_media_lineage');
             CREATE TABLE IF NOT EXISTS media_lineage (
                 parent_media_id  TEXT NOT NULL,
                 child_media_id   TEXT NOT NULL,
                 relation_type    TEXT NOT NULL DEFAULT 'edit',
                 workflow_id      TEXT,
                 created_at       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                 PRIMARY KEY (parent_media_id, child_media_id),
                 FOREIGN KEY (parent_media_id) REFERENCES media(id) ON DELETE CASCADE,
                 FOREIGN KEY (child_media_id) REFERENCES media(id) ON DELETE CASCADE,
                 FOREIGN KEY (workflow_id) REFERENCES comfyui_workflows(id) ON DELETE SET NULL
             );
             CREATE INDEX IF NOT EXISTS idx_lineage_parent ON media_lineage(parent_media_id);
             CREATE INDEX IF NOT EXISTS idx_lineage_child ON media_lineage(child_media_id);",
        )?;
    }

    // 0026_media_source
    {
        let has_source: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('media') WHERE name='source'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !has_source {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0026_media_source');
                 ALTER TABLE media ADD COLUMN source TEXT;",
            )?;
        } else {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0026_media_source');",
            )?;
        }
    }

    // 0027_variant_to_lineage
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0027_variant_to_lineage'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0027_variant_to_lineage');

                 INSERT OR IGNORE INTO media (
                     id, source_path, file_size,
                     width, height, media_type, source, created_at
                 )
                 SELECT
                     v.id,
                     v.file_path,
                     v.file_size,
                     v.width,
                     v.height,
                     COALESCE(v.media_type, 'image'),
                     v.source,
                     v.created_at
                 FROM variants v
                 WHERE NOT EXISTS (SELECT 1 FROM media WHERE id = v.id);

                 INSERT OR IGNORE INTO media_lineage (
                     parent_media_id, child_media_id, relation_type
                 )
                 SELECT
                     v.media_id,
                     v.id,
                     'edit'
                 FROM variants v
                 WHERE EXISTS (SELECT 1 FROM media WHERE id = v.id)
                   AND NOT EXISTS (
                       SELECT 1 FROM media_lineage
                       WHERE parent_media_id = v.media_id AND child_media_id = v.id
                   );

                 UPDATE captions SET media_id = variant_id
                 WHERE variant_id IS NOT NULL
                   AND EXISTS (SELECT 1 FROM media WHERE id = variant_id);

                 UPDATE embeddings SET media_id = variant_id
                 WHERE variant_id IS NOT NULL
                   AND EXISTS (SELECT 1 FROM media WHERE id = variant_id);

                 UPDATE media_tags SET media_id = variant_id
                 WHERE variant_id IS NOT NULL
                   AND EXISTS (SELECT 1 FROM media WHERE id = variant_id);",
            )?;
        }
    }

    // 0028_drop_variants
    // Unconditional: DROP TABLE IF EXISTS is safe to re-run, and we need it here
    // to clean up the variants table that 0003_variants may have just re-created.
    {
        conn.execute_batch(
            "INSERT OR IGNORE INTO _migrations (name) VALUES ('0028_drop_variants');
             DROP TABLE IF EXISTS variants;
             DROP TABLE IF EXISTS variant_presets;",
        )?;
    }

    // 0029_remove_variant_columns
    // Drop FK constraints referencing the (now-dropped) variants table.
    // SQLite with SQLITE_DEFAULT_FOREIGN_KEYS=1 requires the FK parent table to exist at
    // statement-prep time, so we must eliminate these FK constraints entirely.
    // We keep the display_variant_id column on media (without FK) for struct compatibility;
    // variant_id columns are fully removed from captions, embeddings, and media_tags.
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0029_remove_variant_columns'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            // Check whether any variant FK columns remain (e.g. captions.variant_id).
            // If they do, recreate the affected tables without them.
            let has_variant_captions: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('captions') WHERE name='variant_id'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if has_variant_captions {
                conn.execute_batch(
                    "INSERT OR IGNORE INTO _migrations (name) VALUES ('0029_remove_variant_columns');
                     PRAGMA foreign_keys = OFF;

                     /* captions — drop variant_id column (data already migrated via 0027) */
                     CREATE TABLE captions_new (
                         id TEXT PRIMARY KEY,
                         media_id TEXT NOT NULL,
                         text TEXT NOT NULL,
                         source TEXT,
                         created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                     );
                     INSERT INTO captions_new SELECT id, media_id, text, source, created_at, updated_at FROM captions;
                     DROP TABLE captions;
                     ALTER TABLE captions_new RENAME TO captions;
                     CREATE INDEX IF NOT EXISTS idx_captions_media ON captions(media_id);

                     /* embeddings — drop variant_id column */
                     CREATE TABLE embeddings_new (
                         media_id TEXT NOT NULL,
                         model TEXT NOT NULL,
                         content_type TEXT NOT NULL,
                         vector BLOB NOT NULL,
                         created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                     );
                     INSERT INTO embeddings_new SELECT media_id, model, content_type, vector, created_at FROM embeddings;
                     DROP TABLE embeddings;
                     ALTER TABLE embeddings_new RENAME TO embeddings;
                     CREATE UNIQUE INDEX idx_embeddings_unique ON embeddings(media_id, model, content_type);
                     CREATE INDEX idx_embeddings_media ON embeddings(media_id);
                     CREATE INDEX idx_embeddings_model ON embeddings(model);

                     /* media_tags — drop variant_id column */
                     CREATE TABLE media_tags_new (
                         media_id TEXT NOT NULL,
                         tag_id TEXT NOT NULL,
                         confidence REAL,
                         source TEXT,
                         created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         PRIMARY KEY (media_id, tag_id),
                         FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
                         FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
                     );
                     INSERT INTO media_tags_new SELECT media_id, tag_id, confidence, source, created_at FROM media_tags;
                     DROP TABLE media_tags;
                     ALTER TABLE media_tags_new RENAME TO media_tags;
                     CREATE INDEX IF NOT EXISTS idx_media_tags_media ON media_tags(media_id);
                     CREATE INDEX IF NOT EXISTS idx_media_tags_tag ON media_tags(tag_id);

                     /* media — keep display_variant_id column but WITHOUT FK constraint */
                     CREATE TABLE media_new (
                         id TEXT PRIMARY KEY,
                         source_path TEXT,
                         phash BLOB,
                         width INTEGER,
                         height INTEGER,
                         file_size INTEGER,
                         created_at TIMESTAMP,
                         modified_at TIMESTAMP,
                         imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         source_url TEXT,
                         page_url TEXT,
                         source TEXT,
                         deleted_at TEXT,
                         sha256 TEXT,
                         display_variant_id TEXT,
                         lqip TEXT,
                         media_type TEXT DEFAULT 'image',
                         duration REAL,
                         video_codec TEXT,
                         video_fps REAL
                     );
                     INSERT INTO media_new
                     SELECT id, source_path, phash, width, height, file_size,
                            created_at, modified_at, imported_at,
                            source_url, page_url, source,
                            deleted_at, sha256, display_variant_id,
                            lqip, media_type, duration, video_codec, video_fps
                     FROM media;
                     DROP TABLE media;
                     ALTER TABLE media_new RENAME TO media;
                     CREATE INDEX IF NOT EXISTS idx_media_imported_at ON media(imported_at);
                     CREATE INDEX IF NOT EXISTS idx_media_created_at ON media(created_at);
                     CREATE INDEX IF NOT EXISTS idx_media_type ON media(media_type);
                     CREATE INDEX IF NOT EXISTS idx_media_sha256 ON media(sha256);
                     CREATE INDEX IF NOT EXISTS idx_media_deleted_at ON media(deleted_at);
                     CREATE INDEX IF NOT EXISTS idx_media_deleted_imported ON media(deleted_at, imported_at);

                     PRAGMA foreign_keys = ON;",
                )?;
            } else {
                conn.execute_batch(
                    "INSERT OR IGNORE INTO _migrations (name) VALUES ('0029_remove_variant_columns');"
                )?;
            }
        }
    }

    // 0030_fix_remaining_variant_fks
    // Migration 0029 only checked captions.variant_id as the gate, so if an earlier
    // code version had already cleaned up captions, the entire migration (including
    // media_tags and embeddings rebuilds) was skipped. This left FK constraints
    // referencing the now-dropped variants table, causing INSERT failures.
    // 0030 independently checks each table and fixes any remaining variant FKs.
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0030_fix_remaining_variant_fks'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            let media_tags_has_variant: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('media_tags') WHERE name='variant_id'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            let embeddings_has_variant: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('embeddings') WHERE name='variant_id'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            // media.display_variant_id is kept but must lose its FK constraint.
            // We detect the stale FK by checking whether variants table is gone
            // while display_variant_id column still exists.
            let variants_gone: bool = !conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='variants'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(true);
            let media_has_display_variant: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('media') WHERE name='display_variant_id'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            let media_needs_fix = variants_gone && media_has_display_variant;

            if media_tags_has_variant || embeddings_has_variant || media_needs_fix {
                conn.execute_batch("PRAGMA foreign_keys = OFF;")?;

                if media_tags_has_variant {
                    conn.execute_batch(
                        "CREATE TABLE media_tags_new (
                             media_id TEXT NOT NULL,
                             tag_id TEXT NOT NULL,
                             confidence REAL,
                             source TEXT,
                             created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                             PRIMARY KEY (media_id, tag_id),
                             FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
                             FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
                         );
                         INSERT INTO media_tags_new SELECT media_id, tag_id, confidence, source, created_at FROM media_tags;
                         DROP TABLE media_tags;
                         ALTER TABLE media_tags_new RENAME TO media_tags;
                         CREATE INDEX IF NOT EXISTS idx_media_tags_media ON media_tags(media_id);
                         CREATE INDEX IF NOT EXISTS idx_media_tags_tag ON media_tags(tag_id);",
                    )?;
                }

                if embeddings_has_variant {
                    conn.execute_batch(
                        "CREATE TABLE embeddings_new (
                             media_id TEXT NOT NULL,
                             model TEXT NOT NULL,
                             content_type TEXT NOT NULL,
                             vector BLOB NOT NULL,
                             created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                             FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                         );
                         INSERT INTO embeddings_new SELECT media_id, model, content_type, vector, created_at FROM embeddings;
                         DROP TABLE embeddings;
                         ALTER TABLE embeddings_new RENAME TO embeddings;
                         CREATE UNIQUE INDEX IF NOT EXISTS idx_embeddings_unique ON embeddings(media_id, model, content_type);
                         CREATE INDEX IF NOT EXISTS idx_embeddings_media ON embeddings(media_id);
                         CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model);",
                    )?;
                }

                if media_needs_fix {
                    conn.execute_batch(
                        "CREATE TABLE media_new (
                             id TEXT PRIMARY KEY,
                             source_path TEXT,
                             phash BLOB,
                             width INTEGER,
                             height INTEGER,
                             file_size INTEGER,
                             created_at TIMESTAMP,
                             modified_at TIMESTAMP,
                             imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                             source_url TEXT,
                             page_url TEXT,
                             source TEXT,
                             deleted_at TEXT,
                             sha256 TEXT,
                             display_variant_id TEXT,
                             lqip TEXT,
                             media_type TEXT DEFAULT 'image',
                             duration REAL,
                             video_codec TEXT,
                             video_fps REAL
                         );
                         INSERT INTO media_new
                         SELECT id, source_path, phash, width, height, file_size,
                                created_at, modified_at, imported_at,
                                source_url, page_url, source,
                                deleted_at, sha256, display_variant_id,
                                lqip, media_type, duration, video_codec, video_fps
                         FROM media;
                         DROP TABLE media;
                         ALTER TABLE media_new RENAME TO media;
                         CREATE INDEX IF NOT EXISTS idx_media_imported_at ON media(imported_at);
                         CREATE INDEX IF NOT EXISTS idx_media_created_at ON media(created_at);
                         CREATE INDEX IF NOT EXISTS idx_media_type ON media(media_type);
                         CREATE INDEX IF NOT EXISTS idx_media_sha256 ON media(sha256);
                         CREATE INDEX IF NOT EXISTS idx_media_deleted_at ON media(deleted_at);
                         CREATE INDEX IF NOT EXISTS idx_media_deleted_imported ON media(deleted_at, imported_at);",
                    )?;
                }

                conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            }

            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0030_fix_remaining_variant_fks');",
            )?;
        }
    }

    // 0031_fix_media_variant_fk
    // Migration 0030 used display_variant_id column existence as a proxy for the FK,
    // but the FK could still be present on media even if the detection missed it.
    // This migration directly queries pragma_foreign_key_list to check for any
    // remaining FK to the (now-dropped) variants table, and rebuilds media if found.
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0031_fix_media_variant_fk'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            // Check each table that might have a stale FK to the dropped variants table.
            // Use pragma_foreign_key_list for reliable FK detection (not column-name heuristics).
            let has_fk = |tbl: &str| -> bool {
                conn.query_row(
                    &format!("SELECT COUNT(*) > 0 FROM pragma_foreign_key_list('{}') WHERE \"table\" = 'variants'", tbl),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false)
            };

            let media_broken = has_fk("media");
            let captions_broken = has_fk("captions");
            let embeddings_broken = has_fk("embeddings");
            let media_tags_broken = has_fk("media_tags");

            if media_broken || captions_broken || embeddings_broken || media_tags_broken {
                conn.execute_batch("PRAGMA foreign_keys = OFF;")?;

                // media: keep display_variant_id column, drop FK only
                if media_broken {
                    conn.execute_batch(
                        "CREATE TABLE IF NOT EXISTS media_new (
                             id TEXT PRIMARY KEY, source_path TEXT, phash BLOB,
                             width INTEGER, height INTEGER, file_size INTEGER,
                             created_at TIMESTAMP, modified_at TIMESTAMP,
                             imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                             source_url TEXT, page_url TEXT, source TEXT,
                             deleted_at TEXT, sha256 TEXT,
                             display_variant_id TEXT, lqip TEXT,
                             media_type TEXT DEFAULT 'image',
                             duration REAL, video_codec TEXT, video_fps REAL
                         );
                         INSERT INTO media_new SELECT * FROM media;
                         DROP TABLE media;
                         ALTER TABLE media_new RENAME TO media;
                         CREATE INDEX IF NOT EXISTS idx_media_imported_at ON media(imported_at);
                         CREATE INDEX IF NOT EXISTS idx_media_created_at ON media(created_at);
                         CREATE INDEX IF NOT EXISTS idx_media_type ON media(media_type);
                         CREATE INDEX IF NOT EXISTS idx_media_sha256 ON media(sha256);
                         CREATE INDEX IF NOT EXISTS idx_media_deleted_at ON media(deleted_at);
                         CREATE INDEX IF NOT EXISTS idx_media_deleted_imported ON media(deleted_at, imported_at);",
                    )?;
                }

                // captions: drop variant_id column entirely
                if captions_broken {
                    conn.execute_batch(
                        "CREATE TABLE IF NOT EXISTS captions_new (
                             id TEXT PRIMARY KEY, media_id TEXT NOT NULL, text TEXT NOT NULL,
                             source TEXT, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                             updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                             FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                         );
                         INSERT INTO captions_new SELECT id, media_id, text, source, created_at, updated_at FROM captions;
                         DROP TABLE captions;
                         ALTER TABLE captions_new RENAME TO captions;
                         CREATE INDEX IF NOT EXISTS idx_captions_media ON captions(media_id);",
                    )?;
                }

                // embeddings: drop variant_id column entirely
                if embeddings_broken {
                    conn.execute_batch(
                        "CREATE TABLE IF NOT EXISTS embeddings_new (
                             media_id TEXT NOT NULL, model TEXT NOT NULL,
                             content_type TEXT NOT NULL, vector BLOB NOT NULL,
                             created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                             FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                         );
                         INSERT INTO embeddings_new SELECT media_id, model, content_type, vector, created_at FROM embeddings;
                         DROP TABLE embeddings;
                         ALTER TABLE embeddings_new RENAME TO embeddings;
                         CREATE UNIQUE INDEX IF NOT EXISTS idx_embeddings_unique ON embeddings(media_id, model, content_type);
                         CREATE INDEX IF NOT EXISTS idx_embeddings_media ON embeddings(media_id);
                         CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model);",
                    )?;
                }

                // media_tags: drop variant_id column entirely
                if media_tags_broken {
                    conn.execute_batch(
                        "CREATE TABLE IF NOT EXISTS media_tags_new (
                             media_id TEXT NOT NULL, tag_id TEXT NOT NULL,
                             confidence REAL, source TEXT,
                             created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                             PRIMARY KEY (media_id, tag_id),
                             FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
                             FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
                         );
                         INSERT INTO media_tags_new SELECT media_id, tag_id, confidence, source, created_at FROM media_tags;
                         DROP TABLE media_tags;
                         ALTER TABLE media_tags_new RENAME TO media_tags;
                         CREATE INDEX IF NOT EXISTS idx_media_tags_media ON media_tags(media_id);
                         CREATE INDEX IF NOT EXISTS idx_media_tags_tag ON media_tags(tag_id);",
                    )?;
                }

                conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            }

            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0031_fix_media_variant_fk');",
            )?;
        }
    }

    // 0032_robust_variant_fk_cleanup
    // 0031 used pragma_foreign_key_list for detection, but with
    // SQLITE_DEFAULT_FOREIGN_KEYS=1 the pragma query itself can fail with
    // "no such table: variants" when the referenced table is gone.
    // .unwrap_or(false) silently swallows this, skipping the fix.
    // 0032 uses pragma_table_info (safe, no FK validation) with FK OFF as a
    // safeguard, and unconditionally drops any remaining variant FKs.
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0032_robust_variant_fk_cleanup'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            // Disable FK enforcement BEFORE any detection queries, so even if
            // pragma_table_info or the table-recreate steps touch FK metadata,
            // SQLite won't validate that the parent table exists.
            conn.execute_batch("PRAGMA foreign_keys = OFF;")?;

            // Check which tables still have variant_id columns
            let media_tags_has_variant: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('media_tags') WHERE name='variant_id'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            let captions_has_variant: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('captions') WHERE name='variant_id'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            let embeddings_has_variant: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('embeddings') WHERE name='variant_id'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            let media_has_display_variant: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('media') WHERE name='display_variant_id'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            // media_tags: drop variant_id column entirely
            if media_tags_has_variant {
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS media_tags_new_0032 (
                         media_id TEXT NOT NULL,
                         tag_id TEXT NOT NULL,
                         confidence REAL,
                         source TEXT,
                         created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         PRIMARY KEY (media_id, tag_id),
                         FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
                         FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
                     );
                     INSERT OR IGNORE INTO media_tags_new_0032
                         SELECT media_id, tag_id, confidence, source, created_at FROM media_tags;
                     DROP TABLE IF EXISTS media_tags;
                     ALTER TABLE media_tags_new_0032 RENAME TO media_tags;
                     CREATE INDEX IF NOT EXISTS idx_media_tags_media ON media_tags(media_id);
                     CREATE INDEX IF NOT EXISTS idx_media_tags_tag ON media_tags(tag_id);",
                )?;
            }

            // captions: drop variant_id column entirely
            if captions_has_variant {
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS captions_new_0032 (
                         id TEXT PRIMARY KEY,
                         media_id TEXT NOT NULL,
                         text TEXT NOT NULL,
                         source TEXT,
                         created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                     );
                     INSERT OR IGNORE INTO captions_new_0032
                         SELECT id, media_id, text, source, created_at, updated_at FROM captions;
                     DROP TABLE IF EXISTS captions;
                     ALTER TABLE captions_new_0032 RENAME TO captions;
                     CREATE INDEX IF NOT EXISTS idx_captions_media ON captions(media_id);",
                )?;
            }

            // embeddings: drop variant_id column entirely
            if embeddings_has_variant {
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS embeddings_new_0032 (
                         media_id TEXT NOT NULL,
                         model TEXT NOT NULL,
                         content_type TEXT NOT NULL,
                         vector BLOB NOT NULL,
                         created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                     );
                     INSERT OR IGNORE INTO embeddings_new_0032
                         SELECT media_id, model, content_type, vector, created_at FROM embeddings;
                     DROP TABLE IF EXISTS embeddings;
                     ALTER TABLE embeddings_new_0032 RENAME TO embeddings;
                     CREATE UNIQUE INDEX IF NOT EXISTS idx_embeddings_unique ON embeddings(media_id, model, content_type);
                     CREATE INDEX IF NOT EXISTS idx_embeddings_media ON embeddings(media_id);
                     CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model);",
                )?;
            }

            // media: keep display_variant_id column but drop FK constraint
            if media_has_display_variant {
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS media_new_0032 (
                         id TEXT PRIMARY KEY, source_path TEXT, phash BLOB,
                         width INTEGER, height INTEGER, file_size INTEGER,
                         created_at TIMESTAMP, modified_at TIMESTAMP,
                         imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                         source_url TEXT, page_url TEXT, source TEXT,
                         deleted_at TEXT, sha256 TEXT,
                         display_variant_id TEXT, lqip TEXT,
                         media_type TEXT DEFAULT 'image',
                         duration REAL, video_codec TEXT, video_fps REAL
                     );
                     INSERT OR IGNORE INTO media_new_0032 SELECT * FROM media;
                     DROP TABLE IF EXISTS media;
                     ALTER TABLE media_new_0032 RENAME TO media;
                     CREATE INDEX IF NOT EXISTS idx_media_imported_at ON media(imported_at);
                     CREATE INDEX IF NOT EXISTS idx_media_created_at ON media(created_at);
                     CREATE INDEX IF NOT EXISTS idx_media_type ON media(media_type);
                     CREATE INDEX IF NOT EXISTS idx_media_sha256 ON media(sha256);
                     CREATE INDEX IF NOT EXISTS idx_media_deleted_at ON media(deleted_at);
                     CREATE INDEX IF NOT EXISTS idx_media_deleted_imported ON media(deleted_at, imported_at);",
                )?;
            }

            conn.execute_batch("PRAGMA foreign_keys = ON;")?;

            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0032_robust_variant_fk_cleanup');",
            )?;
        }
    }

    // 0033_repair_dangling_variant_fks
    // 0003/0011/0014 could re-attach foreign keys to a `variants` table that 0028
    // had already dropped, and 0029-0032 never ran again once recorded. A database
    // corrupted that way keeps a dangling FK indefinitely: with foreign_keys ON,
    // every INSERT/DELETE against the affected table fails at prepare time with
    // "no such table: main.variants" — which is what broke tagging and trash
    // emptying. This one-shot migration rebuilds any table whose DDL still
    // mentions `variants`.
    {
        let mig_applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = '0033_repair_dangling_variant_fks'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !mig_applied {
            let media_tags_broken = table_has_column(conn, "media_tags", "variant_id")
                || table_ddl_mentions_variants(conn, "media_tags");
            let captions_broken = table_has_column(conn, "captions", "variant_id")
                || table_ddl_mentions_variants(conn, "captions");
            let embeddings_broken = table_has_column(conn, "embeddings", "variant_id")
                || table_ddl_mentions_variants(conn, "embeddings");
            // media deliberately keeps its display_variant_id column (it is still
            // selected into Media::display_variant_id); only the FK goes away.
            let media_broken = table_ddl_mentions_variants(conn, "media");

            if media_tags_broken || captions_broken || embeddings_broken || media_broken {
                // PRAGMA foreign_keys is a no-op inside a transaction, so it has to
                // be toggled outside the one below.
                conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
                {
                    // One transaction for the whole repair: SQLite DDL is
                    // transactional, so a failure rolls back to the original tables
                    // instead of leaving them dropped between DROP and RENAME.
                    let tx = conn.transaction()?;

                    if media_tags_broken {
                        tx.execute_batch(
                            "DROP TABLE IF EXISTS media_tags_new_0033;
                             CREATE TABLE media_tags_new_0033 (
                                 media_id TEXT NOT NULL,
                                 tag_id TEXT NOT NULL,
                                 confidence REAL,
                                 source TEXT,
                                 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                                 PRIMARY KEY (media_id, tag_id),
                                 FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
                                 FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
                             );
                             INSERT INTO media_tags_new_0033
                                 (media_id, tag_id, confidence, source, created_at)
                                 SELECT media_id, tag_id, confidence, source, created_at
                                 FROM media_tags;
                             DROP TABLE media_tags;
                             ALTER TABLE media_tags_new_0033 RENAME TO media_tags;
                             CREATE INDEX IF NOT EXISTS idx_media_tags_media ON media_tags(media_id);
                             CREATE INDEX IF NOT EXISTS idx_media_tags_tag ON media_tags(tag_id);",
                        )?;
                    }

                    if captions_broken {
                        tx.execute_batch(
                            "DROP TABLE IF EXISTS captions_new_0033;
                             CREATE TABLE captions_new_0033 (
                                 id TEXT PRIMARY KEY,
                                 media_id TEXT NOT NULL,
                                 text TEXT NOT NULL,
                                 source TEXT,
                                 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                                 updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                                 FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                             );
                             INSERT INTO captions_new_0033
                                 (id, media_id, text, source, created_at, updated_at)
                                 SELECT id, media_id, text, source, created_at, updated_at
                                 FROM captions;
                             DROP TABLE captions;
                             ALTER TABLE captions_new_0033 RENAME TO captions;
                             CREATE INDEX IF NOT EXISTS idx_captions_media ON captions(media_id);",
                        )?;
                    }

                    if embeddings_broken {
                        tx.execute_batch(
                            "DROP TABLE IF EXISTS embeddings_new_0033;
                             CREATE TABLE embeddings_new_0033 (
                                 media_id TEXT NOT NULL,
                                 model TEXT NOT NULL,
                                 content_type TEXT NOT NULL,
                                 vector BLOB NOT NULL,
                                 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                                 FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
                             );
                             INSERT INTO embeddings_new_0033
                                 (media_id, model, content_type, vector, created_at)
                                 SELECT media_id, model, content_type, vector, created_at
                                 FROM embeddings;
                             DROP TABLE embeddings;
                             ALTER TABLE embeddings_new_0033 RENAME TO embeddings;
                             CREATE UNIQUE INDEX IF NOT EXISTS idx_embeddings_unique ON embeddings(media_id, model, content_type);
                             CREATE INDEX IF NOT EXISTS idx_embeddings_media ON embeddings(media_id);
                             CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model);",
                        )?;
                    }

                    // Rebuilt last: other tables reference media, so its own
                    // rebuild waits until their dangling DDL is gone.
                    if media_broken {
                        tx.execute_batch(
                            "DROP TABLE IF EXISTS media_new_0033;
                             CREATE TABLE media_new_0033 (
                                 id TEXT PRIMARY KEY, source_path TEXT, phash BLOB,
                                 width INTEGER, height INTEGER, file_size INTEGER,
                                 created_at TIMESTAMP, modified_at TIMESTAMP,
                                 imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                                 source_url TEXT, page_url TEXT, source TEXT,
                                 deleted_at TEXT, sha256 TEXT,
                                 display_variant_id TEXT, lqip TEXT,
                                 media_type TEXT DEFAULT 'image',
                                 duration REAL, video_codec TEXT, video_fps REAL
                             );
                             INSERT INTO media_new_0033
                                 SELECT id, source_path, phash, width, height, file_size,
                                        created_at, modified_at, imported_at,
                                        source_url, page_url, source,
                                        deleted_at, sha256, display_variant_id,
                                        lqip, media_type, duration, video_codec, video_fps
                                 FROM media;
                             DROP TABLE media;
                             ALTER TABLE media_new_0033 RENAME TO media;
                             CREATE INDEX IF NOT EXISTS idx_media_imported_at ON media(imported_at);
                             CREATE INDEX IF NOT EXISTS idx_media_created_at ON media(created_at);
                             CREATE INDEX IF NOT EXISTS idx_media_type ON media(media_type);
                             CREATE INDEX IF NOT EXISTS idx_media_sha256 ON media(sha256);
                             CREATE INDEX IF NOT EXISTS idx_media_deleted_at ON media(deleted_at);
                             CREATE INDEX IF NOT EXISTS idx_media_deleted_imported ON media(deleted_at, imported_at);",
                        )?;
                    }

                    tx.commit()?;
                }
                conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            }

            // 0020's index is dropped by every media rebuild and never restored.
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_media_display_variant ON media(display_variant_id);",
            )?;

            // Recorded only once the rebuilds above actually succeeded.
            conn.execute_batch(
                "INSERT OR IGNORE INTO _migrations (name) VALUES ('0033_repair_dangling_variant_fks');",
            )?;
        }
    }

    Ok(())
}

/// True when the live DDL of `table` mentions the legacy `variants` table.
///
/// Reads `sqlite_master.sql` rather than `pragma_foreign_key_list`, because the
/// latter resolves the foreign-key parent and fails with "no such table:
/// variants" — the very failure this module is trying to recover from.
fn table_ddl_mentions_variants(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master
         WHERE type = 'table' AND name = ?1 AND sql LIKE '%variants%'",
        params![table],
        |row| row.get(0),
    )
    .unwrap_or(false)
}

/// True when `table` currently has a column named `column`.
fn table_has_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.query_row(
        &format!(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('{}') WHERE name = ?1",
            table
        ),
        params![column],
        |row| row.get(0),
    )
    .unwrap_or(false)
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
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

pub fn collection_get(
    app: &AppHandle,
    id: &str,
) -> Result<Option<Collection>, Box<dyn std::error::Error>> {
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
    if let Some(r) = rows.next() {
        return Ok(Some(r?));
    }
    Ok(None)
}

pub fn collection_create(
    app: &AppHandle,
    name: &str,
    description: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let id = ulid::Ulid::new().to_string();
    let desc = if description.is_empty() {
        None
    } else {
        Some(description.to_string())
    };
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

pub fn collection_rename(
    app: &AppHandle,
    id: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "UPDATE collections SET name = ?2 WHERE id = ?1",
        params![id, name],
    )?;
    Ok(())
}

pub fn collection_pin(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "UPDATE collections SET pinned_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn collection_unpin(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "UPDATE collections SET pinned_at = NULL WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn collection_add_item(
    app: &AppHandle,
    collection_id: &str,
    media_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "INSERT OR IGNORE INTO collection_items (collection_id, media_id) VALUES (?1, ?2)",
        params![collection_id, media_id],
    )?;
    Ok(())
}

pub fn collection_add_batch(
    app: &AppHandle,
    collection_id: &str,
    media_ids: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
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
        Ok(()) => {
            conn.execute("COMMIT", [])?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", []);
            Err(e)
        }
    }
}

pub fn collection_remove_item(
    app: &AppHandle,
    collection_id: &str,
    media_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
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
                m.display_variant_id, m.lqip,
                m.media_type, m.duration, m.video_codec, m.video_fps
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
            media_type: row.get(15)?,
            duration: row.get(16)?,
            video_codec: row.get(17)?,
            video_fps: row.get(18)?,
            thumb_256: None,
        })
    })?;
    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

pub fn collection_get_item_ids(
    app: &AppHandle,
    collection_id: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt =
        conn.prepare("SELECT media_id FROM collection_items WHERE collection_id = ?1")?;
    let rows = stmt.query_map(params![collection_id], |row| row.get::<_, String>(0))?;
    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

pub fn collection_first_media_id(
    app: &AppHandle,
    collection_id: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare("SELECT media_id FROM collection_items WHERE collection_id = ?1 ORDER BY created_at LIMIT 1")?;
    let mut rows = stmt.query_map(params![collection_id], |row| row.get::<_, String>(0))?;
    if let Some(r) = rows.next() {
        return Ok(Some(r?));
    }
    Ok(None)
}

pub fn media_get_by_sha256(
    app: &AppHandle,
    hash: &str,
) -> Result<Option<Media>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT id, source_path, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, deleted_at, display_variant_id, lqip,
                media_type, duration, video_codec, video_fps
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
            media_type: row.get(15)?,
            duration: row.get(16)?,
            video_codec: row.get(17)?,
            video_fps: row.get(18)?,
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
        "INSERT INTO media (id, source_path, phash, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, lqip, media_type, duration, video_codec, video_fps)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
            media.media_type.as_ref(),
            media.duration,
            media.video_codec.as_ref(),
            media.video_fps,
        ],
    )?;
    Ok(())
}

pub(crate) fn resolve_thumb_paths(app: &AppHandle, media_list: &mut [Media]) {
    let Ok(app_dir) = app.path().app_data_dir() else {
        return;
    };
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
        "SELECT id, source_path, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, deleted_at, display_variant_id, lqip,
                media_type, duration, video_codec, video_fps
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
            media_type: row.get(15)?,
            duration: row.get(16)?,
            video_codec: row.get(17)?,
            video_fps: row.get(18)?,
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

// --- Browse item queries (lineage root filter) ---

pub fn list_browse_items_path(
    db_path: &Path,
    sort_by: &str,
    descending: bool,
    offset: u32,
    limit: u32,
    visibility: &BrowseVisibility,
) -> Result<Vec<BrowseItem>, Box<dyn std::error::Error>> {
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

    let root_filter = match visibility {
        BrowseVisibility::Representative => {
            "AND m.id NOT IN (SELECT child_media_id FROM media_lineage)"
        }
        BrowseVisibility::All => "",
    };

    let sql = format!(
        "SELECT
            m.id,
            m.source_path,
            m.width,
            m.height,
            m.file_size,
            m.created_at,
            m.modified_at,
            m.imported_at,
            m.source_url,
            m.page_url,
            m.source,
            m.sha256,
            m.deleted_at,
            m.lqip,
            m.media_type,
            m.duration,
            m.video_codec,
            m.video_fps,
            (SELECT COUNT(*) FROM media_lineage WHERE parent_media_id = m.id) AS child_count,
            (SELECT COUNT(*) FROM media_lineage WHERE child_media_id = m.id) AS parent_count
        FROM media m
        WHERE m.deleted_at IS NULL
        {}
        ORDER BY m.{} {}
        LIMIT ? OFFSET ?",
        root_filter, sort_column, order
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
        Ok(BrowseItem {
            id: row.get(0)?,
            media_id: row.get(0)?,
            source_path: row.get(1)?,
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
            lqip: row.get(13)?,
            media_type: row.get(14)?,
            duration: row.get(15)?,
            video_codec: row.get(16)?,
            video_fps: row.get(17)?,
            child_count: row.get(18)?,
            parent_count: row.get(19)?,
            thumb_256: None,
        })
    })?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

pub fn list_browse_items(
    app: &AppHandle,
    sort_by: &str,
    descending: bool,
    offset: u32,
    limit: u32,
    visibility: &BrowseVisibility,
) -> Result<Vec<BrowseItem>, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let mut results =
        list_browse_items_path(&path, sort_by, descending, offset, limit, visibility)?;
    resolve_browse_thumb_paths(app, &mut results);
    Ok(results)
}

pub fn browse_count_path(
    db_path: &Path,
    visibility: &BrowseVisibility,
) -> Result<u32, Box<dyn std::error::Error>> {
    let conn = Connection::open(db_path)?;
    let root_filter = match visibility {
        BrowseVisibility::Representative => {
            "AND m.id NOT IN (SELECT child_media_id FROM media_lineage)"
        }
        BrowseVisibility::All => "",
    };
    let sql = format!(
        "SELECT COUNT(*) FROM media m WHERE m.deleted_at IS NULL {}",
        root_filter
    );
    let count: u32 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count)
}

/// Browse query filtered to specific media_ids.
/// Used by search and collection browsing to expand candidate media into browse items.
pub fn browse_query_filtered_path(
    db_path: &Path,
    media_ids: &[String],
    sort_by: &str,
    descending: bool,
    offset: u32,
    limit: u32,
    visibility: &BrowseVisibility,
) -> Result<Vec<BrowseItem>, Box<dyn std::error::Error>> {
    if media_ids.is_empty() {
        return Ok(vec![]);
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

    let root_filter = match visibility {
        BrowseVisibility::Representative => {
            "AND m.id NOT IN (SELECT child_media_id FROM media_lineage)"
        }
        BrowseVisibility::All => "",
    };

    let in_clause = media_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    let sql = format!(
        "SELECT
            m.id,
            m.source_path,
            m.width,
            m.height,
            m.file_size,
            m.created_at,
            m.modified_at,
            m.imported_at,
            m.source_url,
            m.page_url,
            m.source,
            m.sha256,
            m.deleted_at,
            m.lqip,
            m.media_type,
            m.duration,
            m.video_codec,
            m.video_fps,
            (SELECT COUNT(*) FROM media_lineage WHERE parent_media_id = m.id) AS child_count,
            (SELECT COUNT(*) FROM media_lineage WHERE child_media_id = m.id) AS parent_count
        FROM media m
        WHERE m.deleted_at IS NULL
          AND m.id IN ({})
        {}
        ORDER BY m.{} {}
        LIMIT ? OFFSET ?",
        in_clause, root_filter, sort_column, order
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut param_refs: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    for id in media_ids {
        param_refs.push(Box::new(id.clone()));
    }
    param_refs.push(Box::new(limit as i64));
    param_refs.push(Box::new(offset as i64));

    let param_slice: Vec<&dyn rusqlite::types::ToSql> =
        param_refs.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_slice.as_slice(), |row| {
        Ok(BrowseItem {
            id: row.get(0)?,
            media_id: row.get(0)?,
            source_path: row.get(1)?,
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
            lqip: row.get(13)?,
            media_type: row.get(14)?,
            duration: row.get(15)?,
            video_codec: row.get(16)?,
            video_fps: row.get(17)?,
            child_count: row.get(18)?,
            parent_count: row.get(19)?,
            thumb_256: None,
        })
    })?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r?);
    }
    Ok(results)
}

pub fn browse_query_filtered(
    app: &AppHandle,
    media_ids: &[String],
    sort_by: &str,
    descending: bool,
    offset: u32,
    limit: u32,
    visibility: &BrowseVisibility,
) -> Result<Vec<BrowseItem>, Box<dyn std::error::Error>> {
    let path = db_path(app);
    let mut results = browse_query_filtered_path(
        &path, media_ids, sort_by, descending, offset, limit, visibility,
    )?;
    resolve_browse_thumb_paths(app, &mut results);
    Ok(results)
}

/// Given browse items and tag names, return the set of item IDs that directly have those tags.
pub fn find_items_with_tags(
    app: &AppHandle,
    items: &[BrowseItem],
    tag_names: &[String],
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let fuzzy = crate::settings::is_tag_search_fuzzy(app);
    let conn = get_conn(app)?;
    let media_ids: Vec<&str> = items.iter().map(|it| it.media_id.as_str()).collect();
    find_items_with_tags_inner(&conn, media_ids.as_slice(), tag_names, fuzzy)
}

fn find_items_with_tags_inner(
    conn: &Connection,
    media_ids: &[&str],
    tag_names: &[String],
    fuzzy: bool,
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    if tag_names.is_empty() || media_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let placeholders: Vec<String> = media_ids.iter().map(|_| "?".to_string()).collect();
    let name_condition: String = if fuzzy {
        tag_names
            .iter()
            .map(|_| "t.name LIKE ?")
            .collect::<Vec<_>>()
            .join(" OR ")
    } else {
        let ph = tag_names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        format!("t.name IN ({})", ph)
    };
    let sql = format!(
        "SELECT DISTINCT mt.media_id
         FROM media_tags mt
         JOIN tags t ON mt.tag_id = t.id
         WHERE ({}) AND mt.media_id IN ({})",
        name_condition,
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    for tn in tag_names {
        if fuzzy {
            params.push(Box::new(format!("%{}%", tn)));
        } else {
            params.push(Box::new(tn.clone()));
        }
    }
    for mid in media_ids {
        params.push(Box::new(mid.to_string()));
    }
    let param_slice: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_slice.as_slice(), |row| row.get::<_, String>(0))?;
    let mut result = HashSet::new();
    for r in rows {
        result.insert(r?);
    }
    Ok(result)
}

/// Path-based variant for CLI use (always exact match; fuzzy is frontend-only)
pub fn find_items_with_tags_path(
    db_path: &Path,
    items: &[BrowseItem],
    tag_names: &[String],
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let conn = Connection::open(db_path)?;
    let media_ids: Vec<&str> = items.iter().map(|it| it.media_id.as_str()).collect();
    find_items_with_tags_inner(&conn, media_ids.as_slice(), tag_names, false)
}

pub(crate) fn resolve_browse_thumb_paths(app: &AppHandle, items: &mut [BrowseItem]) {
    let Ok(app_dir) = app.path().app_data_dir() else {
        return;
    };
    let thumbs_dir = app_dir.join("thumbnails");
    for item in items.iter_mut() {
        item.thumb_256 = Some(
            thumbs_dir
                .join(format!("{}_256.jpg", item.id))
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
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
        "SELECT id, source_path, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, deleted_at, display_variant_id, lqip,
                media_type, duration, video_codec, video_fps
         FROM media WHERE deleted_at IS NULL AND id IN ({})",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
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
            media_type: row.get(15)?,
            duration: row.get(16)?,
            video_codec: row.get(17)?,
            video_fps: row.get(18)?,
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
         ORDER BY t.name",
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

pub fn tag_create(app: &AppHandle, name: &str) -> Result<String, Box<dyn std::error::Error>> {
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

pub fn tag_rename(app: &AppHandle, id: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
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
    media_tags_get_by_media_id(app, media_id)
}

pub fn media_tags_get_by_media_id(
    app: &AppHandle,
    media_id: &str,
) -> Result<Vec<Tag>, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, mt.source, mt.confidence FROM tags t
         JOIN media_tags mt ON t.id = mt.tag_id
         WHERE mt.media_id = ?1
         ORDER BY t.name",
    )?;
    let tag_iter = stmt.query_map(params![media_id], |row| {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            source: row.get(2)?,
            confidence: row.get(3)?,
            item_count: None,
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
    media_tag_add_with_source(app, media_id, tag_id, None, None)
}

pub fn media_tag_add_with_source(
    app: &AppHandle,
    media_id: &str,
    tag_id: &str,
    confidence: Option<f64>,
    source: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    media_tag_add_internal(app, media_id, tag_id, confidence, source)
}

fn media_tag_add_internal(
    app: &AppHandle,
    media_id: &str,
    tag_id: &str,
    confidence: Option<f64>,
    source: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "INSERT OR REPLACE INTO media_tags (media_id, tag_id, confidence, source) VALUES (?1, ?2, ?3, ?4)",
        params![media_id, tag_id, confidence, source],
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
        Err(e) => {
            let _ = conn.execute("ROLLBACK", []);
            Err(e)
        }
    }
}

pub fn media_tag_remove(
    app: &AppHandle,
    media_id: &str,
    tag_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    media_tag_remove_for_media(app, media_id, tag_id)
}

pub fn media_tag_remove_for_media(
    app: &AppHandle,
    media_id: &str,
    tag_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    conn.execute(
        "DELETE FROM media_tags WHERE media_id = ?1 AND tag_id = ?2",
        params![media_id, tag_id],
    )?;
    let mid = media_id.to_string();
    drop(conn);
    let _ = fts_sync(app, &mid);
    Ok(())
}

pub fn media_tags_clear(app: &AppHandle, media_id: &str) -> Result<(), Box<dyn std::error::Error>> {
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
    let placeholders: Vec<String> = media_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect();
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
    let params: Vec<&dyn rusqlite::types::ToSql> = media_ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
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
    fuzzy: bool,
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

    let name_condition: String = if fuzzy {
        tag_names
            .iter()
            .map(|_| "t.name LIKE ?")
            .collect::<Vec<_>>()
            .join(" OR ")
    } else {
        let placeholders = tag_names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        format!("t.name IN ({})", placeholders)
    };

    let sql = match mode {
        TagSearchMode::Intersection => format!(
            "SELECT m.id, m.source_path, m.width, m.height, m.file_size, m.created_at, m.modified_at, m.imported_at, m.source_url, m.page_url, m.source, m.sha256, m.deleted_at, m.display_variant_id, m.lqip,
                    m.media_type, m.duration, m.video_codec, m.video_fps
             FROM media m
             JOIN media_tags mt ON m.id = mt.media_id
             JOIN tags t ON mt.tag_id = t.id
             WHERE m.deleted_at IS NULL AND ({})
             GROUP BY m.id
             HAVING COUNT(DISTINCT t.id) = {}
             ORDER BY m.{} {}",
            name_condition, tag_names.len(), sort_column, order
        ),
        TagSearchMode::Union => format!(
            "SELECT DISTINCT m.id, m.source_path, m.width, m.height, m.file_size, m.created_at, m.modified_at, m.imported_at, m.source_url, m.page_url, m.source, m.sha256, m.deleted_at, m.display_variant_id, m.lqip,
                    m.media_type, m.duration, m.video_codec, m.video_fps
             FROM media m
             JOIN media_tags mt ON m.id = mt.media_id
             JOIN tags t ON mt.tag_id = t.id
             WHERE m.deleted_at IS NULL AND ({})
             ORDER BY m.{} {}",
            name_condition, sort_column, order
        ),
    };

    let mut stmt = conn.prepare(&sql)?;
    let params_vec: Vec<String> = if fuzzy {
        tag_names.iter().map(|n| format!("%{}%", n)).collect()
    } else {
        tag_names.to_vec()
    };
    let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec
        .iter()
        .map(|n| n as &dyn rusqlite::ToSql)
        .collect();
    let media_iter = stmt.query_map(param_refs.as_slice(), |row| {
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
            media_type: row.get(15)?,
            duration: row.get(16)?,
            video_codec: row.get(17)?,
            video_fps: row.get(18)?,
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
        let mut stmt = conn.prepare("SELECT text FROM captions WHERE media_id = ?1")?;
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
    conn.execute(
        "DELETE FROM media_fts WHERE media_id = ?1",
        params![media_id],
    )?;
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
    let fuzzy = crate::settings::is_tag_search_fuzzy(app);
    let mut results =
        media_search_by_tags_path(&path, tag_names, sort_by, descending, mode, fuzzy)?;
    resolve_thumb_paths(app, &mut results);
    Ok(results)
}

pub fn media_query_filtered_path(
    db_path: &Path,
    media_ids: Option<&[String]>,
    dimensions: &[crate::search::parser::DimFilter],
    date_range: &Option<crate::search::parser::DateRange>,
    file_size: &Option<crate::search::parser::SizeFilter>,
    media_type: &Option<String>,
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
                conditions.push(format!("{} BETWEEN ?{} AND ?{}", col, n - 1, n));
            }
        }
    }

    if let Some(dr) = date_range {
        bind_values.push(rusqlite::types::Value::Text(dr.start.clone()));
        bind_values.push(rusqlite::types::Value::Text(dr.end.clone()));
        let n = bind_values.len();
        conditions.push(format!("m.created_at BETWEEN ?{} AND ?{}", n - 1, n));
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

    if let Some(ref mt) = *media_type {
        conditions.push(format!("m.media_type = '{}'", mt));
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
                m.display_variant_id, m.lqip,
                m.media_type, m.duration, m.video_codec, m.video_fps
         FROM media m {} ORDER BY {} {}",
        where_clause, sort_column, order
    );

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = bind_values
        .iter()
        .map(|p| p as &dyn rusqlite::types::ToSql)
        .collect();

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
            media_type: row.get(15)?,
            duration: row.get(16)?,
            video_codec: row.get(17)?,
            video_fps: row.get(18)?,
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
    media_type: &Option<String>,
    sort_by: &str,
    descending: bool,
) -> Result<Vec<Media>, Box<dyn std::error::Error>> {
    media_query_filtered_path(
        &db_path(app),
        media_ids,
        dimensions,
        date_range,
        file_size,
        media_type,
        sort_by,
        descending,
    )
}

// --- LineageGraph / LineageEdge structs ---

#[derive(Debug, Clone, Serialize)]
pub struct LineageGraph {
    pub parents: Vec<LineageEdge>,
    pub children: Vec<LineageEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineageEdge {
    pub media_id: String,
    pub relation_type: String,
    pub workflow_id: Option<String>,
    pub created_at: String,
}

// --- media_lineage operations ---

pub fn lineage_insert_path(
    conn: &Connection,
    parent_media_id: &str,
    child_media_id: &str,
    relation_type: &str,
    workflow_id: Option<&str>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO media_lineage (parent_media_id, child_media_id, relation_type, workflow_id)
         VALUES (?1, ?2, ?3, ?4)",
        params![parent_media_id, child_media_id, relation_type, workflow_id],
    )?;
    Ok(())
}

pub fn lineage_insert(
    app: &AppHandle,
    parent_media_id: &str,
    child_media_id: &str,
    relation_type: &str,
    workflow_id: Option<&str>,
) -> Result<(), String> {
    let conn = get_conn(app)?;
    lineage_insert_path(
        &conn,
        parent_media_id,
        child_media_id,
        relation_type,
        workflow_id,
    )
    .map_err(|e| e.to_string())
}

pub fn lineage_list_path(
    conn: &Connection,
    media_id: &str,
) -> Result<LineageGraph, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT parent_media_id, relation_type, workflow_id, created_at
         FROM media_lineage WHERE child_media_id = ?1 ORDER BY created_at",
    )?;
    let parents: Vec<LineageEdge> = stmt
        .query_map(params![media_id], |r| {
            Ok(LineageEdge {
                media_id: r.get(0)?,
                relation_type: r.get(1)?,
                workflow_id: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut stmt = conn.prepare(
        "SELECT child_media_id, relation_type, workflow_id, created_at
         FROM media_lineage WHERE parent_media_id = ?1 ORDER BY created_at",
    )?;
    let children: Vec<LineageEdge> = stmt
        .query_map(params![media_id], |r| {
            Ok(LineageEdge {
                media_id: r.get(0)?,
                relation_type: r.get(1)?,
                workflow_id: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(LineageGraph { parents, children })
}

pub fn lineage_list(app: &AppHandle, media_id: &str) -> Result<LineageGraph, String> {
    let conn = get_conn(app)?;
    lineage_list_path(&conn, media_id).map_err(|e| e.to_string())
}

pub fn lineage_remove_path(
    conn: &Connection,
    parent_media_id: &str,
    child_media_id: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM media_lineage WHERE parent_media_id = ?1 AND child_media_id = ?2",
        params![parent_media_id, child_media_id],
    )?;
    Ok(())
}

pub fn lineage_remove(
    app: &AppHandle,
    parent_media_id: &str,
    child_media_id: &str,
) -> Result<(), String> {
    let conn = get_conn(app)?;
    lineage_remove_path(&conn, parent_media_id, child_media_id).map_err(|e| e.to_string())
}

/// Returns true if inserting (parent → child) would create a cycle in the DAG.
pub fn lineage_would_cycle_path(
    conn: &Connection,
    parent_id: &str,
    child_id: &str,
) -> Result<bool, rusqlite::Error> {
    use std::collections::HashSet;
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = vec![child_id.to_string()];

    while let Some(current) = queue.pop() {
        if current == parent_id {
            return Ok(true);
        }
        if visited.insert(current.clone()) {
            let mut stmt = conn
                .prepare("SELECT child_media_id FROM media_lineage WHERE parent_media_id = ?1")?;
            let descendants: Vec<String> = stmt
                .query_map(params![current], |r| r.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            queue.extend(descendants);
        }
    }

    Ok(false)
}

pub fn lineage_insert_safe(
    app: &AppHandle,
    parent_media_id: &str,
    child_media_id: &str,
    relation_type: &str,
    workflow_id: Option<&str>,
) -> Result<(), String> {
    let conn = get_conn(app)?;
    if lineage_would_cycle_path(&conn, parent_media_id, child_media_id)
        .map_err(|e| e.to_string())?
    {
        return Err("Adding this lineage would create a cycle".into());
    }
    lineage_insert_path(
        &conn,
        parent_media_id,
        child_media_id,
        relation_type,
        workflow_id,
    )
    .map_err(|e| e.to_string())
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
        "SELECT id, media_id, text, source, created_at, updated_at
         FROM captions WHERE media_id = ?1 ORDER BY created_at DESC",
    )?;
    let caption_iter = stmt.query_map(params![media_id], |row| {
        Ok(Caption {
            id: row.get(0)?,
            media_id: row.get(1)?,
            text: row.get(2)?,
            source: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
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

pub fn caption_create_with_source(
    app: &AppHandle,
    media_id: &str,
    text: &str,
    source: Option<&str>,
) -> Result<Caption, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let id = Ulid::new().to_string();
    conn.execute(
        "INSERT INTO captions (id, media_id, text, source) VALUES (?1, ?2, ?3, ?4)",
        params![&id, media_id, text, source],
    )?;
    let mid = media_id.to_string();
    drop(conn);
    let _ = fts_sync(app, &mid);
    Ok(Caption {
        id,
        media_id: media_id.to_string(),
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
) -> Result<String, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
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
    Ok(mid)
}

pub fn caption_delete(app: &AppHandle, id: &str) -> Result<String, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let mid: String = conn.query_row(
        "SELECT media_id FROM captions WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    conn.execute("DELETE FROM captions WHERE id = ?1", params![id])?;
    drop(conn);
    let _ = fts_sync(app, &mid);
    Ok(mid)
}

/// Look up media_id for a caption without modifying it.
pub fn caption_get_media_info(
    app: &AppHandle,
    id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    Ok(conn.query_row(
        "SELECT media_id FROM captions WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?)
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
    // Delete-then-insert to avoid unique constraint violations
    conn.execute(
        "DELETE FROM embeddings WHERE media_id=?1 AND model=?2 AND content_type=?3",
        params![media_id, model, content_type],
    )?;
    conn.execute(
        "INSERT INTO embeddings (media_id, model, content_type, vector) VALUES (?1, ?2, ?3, ?4)",
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

pub fn embedding_clear_all(app: &AppHandle) -> Result<usize, Box<dyn std::error::Error>> {
    let conn = get_conn(app)?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))?;
    conn.execute("DELETE FROM embeddings", [])?;
    Ok(count as usize)
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
    let rows = stmt.query_map(params![media_id], row_mapper)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn row_mapper(row: &rusqlite::Row) -> rusqlite::Result<EmbeddingInfo> {
    Ok(EmbeddingInfo {
        model: row.get(0)?,
        content_type: row.get(1)?,
        vec_dim: row.get(2)?,
        created_at: row.get(3)?,
    })
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

pub fn saved_filters_get_all(
    app: &AppHandle,
) -> Result<Vec<SavedFilter>, Box<dyn std::error::Error>> {
    let json = setting_get(app, "saved_filters")?.unwrap_or_else(|| "[]".to_string());
    Ok(serde_json::from_str(&json)?)
}

pub fn saved_filters_save(
    app: &AppHandle,
    filter: &SavedFilter,
) -> Result<(), Box<dyn std::error::Error>> {
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
    let filters: Vec<SavedFilter> = filters.into_iter().filter(|f| f.name != name).collect();
    setting_set(app, "saved_filters", &serde_json::to_string(&filters)?)
}

// --- Settings operations ---

pub fn setting_get(
    app: &AppHandle,
    key: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
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

pub fn media_permanent_delete(app: &AppHandle, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let app_dir = app.path().app_data_dir().expect("app data dir");
    delete_media_files(&app_dir, id);

    // Delete DB record (cascades to tags/captions/embeddings/lineage)
    let conn = get_conn(app)?;
    conn.execute("DELETE FROM media WHERE id = ?1", params![id])?;

    Ok(())
}

/// True when `file_name` is the on-disk image belonging to media `id`.
///
/// Either the media's own file (`{id}.jpg`) or a derivative written by an older
/// build as `{parent_id}_{id}.png`. Ids are ULIDs — fixed-length Crockford
/// base32 with no underscore — so splitting on the last `_` is unambiguous and
/// `{parent}_{child}` is only ever claimed by `child`.
pub(crate) fn is_file_for_media(file_name: &str, id: &str) -> bool {
    let stem = file_name
        .rsplit_once('.')
        .map(|(stem, _ext)| stem)
        .unwrap_or(file_name);
    stem == id || stem.ends_with(&format!("_{}", id))
}

/// Remove every on-disk artifact of media `id`: its image (from `library/` and
/// the legacy `variants/`) and both thumbnails.
///
/// Note this deliberately does NOT touch `{id}_{child}.ext` — that file belongs
/// to the derivative, whose own media row survives a parent's deletion.
fn delete_media_files(app_dir: &Path, id: &str) {
    for dir in ["library", "variants"] {
        let Ok(entries) = std::fs::read_dir(app_dir.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if is_file_for_media(name, id) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    for suffix in &["256", "512"] {
        let thumb = app_dir
            .join("thumbnails")
            .join(format!("{}_{}.jpg", id, suffix));
        let _ = std::fs::remove_file(&thumb);
    }
}

/// True for `source_path` values that are not local files (browser-plugin imports).
fn is_remote_url(path: &str) -> bool {
    ["http://", "https://", "asset://", "file://"]
        .iter()
        .any(|scheme| path.starts_with(scheme))
}

fn file_stem(file_name: &str) -> &str {
    file_name
        .rsplit_once('.')
        .map(|(stem, _ext)| stem)
        .unwrap_or(file_name)
}

/// First file in `dir` belonging to media `id`, preferring an exact `{id}.{ext}`
/// name over a legacy `{parent}_{id}.{ext}` one so the result does not depend on
/// `read_dir` order.
fn find_file_in_dir(dir: &Path, id: &str) -> Option<PathBuf> {
    let mut legacy: Option<PathBuf> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_file_for_media(name, id) {
            continue;
        }
        if file_stem(name) == id {
            return Some(entry.path());
        }
        if legacy.is_none() {
            legacy = Some(entry.path());
        }
    }
    legacy
}

/// The on-disk file backing media `id`.
///
/// The library copy is authoritative — thumbnails, export, the viewer and AI
/// editing all read it — so it wins over `source_path`. For locally imported
/// media `source_path` still records the *original* location the file was
/// imported from, which may have moved or been deleted since, so it is only a
/// fallback for media whose library copy is gone.
pub fn resolve_media_file_path(
    app_dir: &Path,
    conn: &Connection,
    id: &str,
) -> Result<PathBuf, String> {
    for dir in ["library", "variants"] {
        if let Some(path) = find_file_in_dir(&app_dir.join(dir), id) {
            return Ok(path);
        }
    }

    let source_path: Option<String> = conn
        .query_row(
            "SELECT source_path FROM media WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .ok()
        .flatten();

    if let Some(source_path) = source_path {
        if !source_path.is_empty() && !is_remote_url(&source_path) {
            let path = PathBuf::from(&source_path);
            if path.exists() {
                return Ok(path);
            }
        }
        if is_remote_url(&source_path) {
            return Err(format!(
                "该媒体是网络链接（{}）且未在本地 library 中找到文件，无法用于本地图像编辑",
                source_path
            ));
        }
    }

    Err(format!("未找到媒体 {} 的本地文件", id))
}

/// Tauri-facing wrapper around [`resolve_media_file_path`].
pub fn resolve_media_file(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let conn = get_conn(app)?;
    resolve_media_file_path(&app_dir, &conn, id)
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
        "SELECT id, source_path, width, height, file_size, created_at, modified_at, imported_at, source_url, page_url, source, sha256, deleted_at, display_variant_id, lqip,
                media_type, duration, video_codec, video_fps
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
            media_type: row.get(15)?,
            duration: row.get(16)?,
            video_codec: row.get(17)?,
            video_fps: row.get(18)?,
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

    let mut deleted = 0usize;
    for id in &ids {
        match media_permanent_delete(app, id) {
            Ok(()) => deleted += 1,
            Err(e) => eprintln!("[trash] failed to permanently delete {}: {}", id, e),
        }
    }

    Ok(deleted)
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
                let ar_ratio = if ar_i > ar_j {
                    ar_i / ar_j
                } else {
                    ar_j / ar_i
                };
                if ar_ratio > 2.0 {
                    continue;
                }
                // Pre-filter: skip if file sizes differ by > 4x
                let fs_i = items[i].file_size.max(1) as f64;
                let fs_j = items[j].file_size.max(1) as f64;
                let fs_ratio = if fs_i > fs_j {
                    fs_i / fs_j
                } else {
                    fs_j / fs_i
                };
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

pub fn media_get_by_id(
    app: &AppHandle,
    id: &str,
) -> Result<Option<Media>, Box<dyn std::error::Error>> {
    let list = media_get_batch(app, &[id.to_string()])?;
    Ok(list.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn new_test_db() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        setup_test_db(&db_path).expect("setup_test_db");
        (dir, db_path)
    }

    fn open(path: &Path) -> Connection {
        Connection::open(path).expect("open test db")
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn test_migrations_idempotent() {
        let (_dir, db_path) = new_test_db();
        let mut conn = open(&db_path);

        // Count tables before re-running
        let before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        // Re-run migrations — should not error
        run_migrations(&mut conn).expect("second run_migrations");

        let after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(before, after, "migrations should be idempotent");
    }

    /// Regression guard for the `variants` saga: migrations 0003/0011/0013/0014
    /// used to re-attach foreign keys to the `variants` table after 0028 dropped
    /// it, leaving a dangling FK that broke INSERT/DELETE on `media_tags` and
    /// therefore tagging and trash emptying. A single extra boot was enough.
    #[test]
    fn test_no_variants_references_after_repeated_migrations() {
        let (_dir, db_path) = new_test_db();
        let mut conn = open(&db_path);

        run_migrations(&mut conn).expect("second run_migrations");
        run_migrations(&mut conn).expect("third run_migrations");

        // No table DDL may mention `variants` — that is strictly stronger than
        // checking pragma_foreign_key_list, which cannot even be queried reliably
        // while the parent table is missing.
        let mentioning: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND sql LIKE '%variants%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mentioning, 0, "no table should reference `variants`");

        let variants_table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='variants'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(variants_table, 0, "`variants` must stay dropped");

        assert!(!table_has_column(&conn, "media_tags", "variant_id"));
        assert!(!table_has_column(&conn, "captions", "variant_id"));
        assert!(!table_has_column(&conn, "embeddings", "variant_id"));
        // media keeps the column, only its FK is gone.
        assert!(table_has_column(&conn, "media", "display_variant_id"));
    }

    /// The user-visible symptom: `media_permanent_delete` runs
    /// `DELETE FROM media` on a connection with foreign keys enabled.
    #[test]
    fn test_media_delete_with_fk_on_after_migrations() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);

        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at)
             VALUES ('m1', '/tmp/x.png', 10, 10, 100, '2026-01-01T00:00:00')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO tags (id, name) VALUES ('t1', 'cat')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO media_tags (media_id, tag_id) VALUES ('m1', 't1')",
            [],
        )
        .unwrap();

        // Pre-fix this failed with "in prepare, no such table: main.variants".
        conn.execute("DELETE FROM media WHERE id = 'm1'", [])
            .expect("DELETE FROM media must succeed");

        assert_eq!(count(&conn, "media"), 0);
        assert_eq!(count(&conn, "media_tags"), 0, "cascade must still work");
    }

    /// Repairs a database already corrupted by the 0003/0014 interplay, and
    /// must not lose any rows while rebuilding the tables.
    #[test]
    fn test_repairs_corrupted_legacy_state() {
        let (_dir, db_path) = new_test_db();
        let mut conn = open(&db_path);

        // Seed real data first so we can prove the rebuild preserves it.
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at)
             VALUES ('m1', '/tmp/x.png', 10, 10, 100, '2026-01-01T00:00:00')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO tags (id, name) VALUES ('t1', 'cat')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO media_tags (media_id, tag_id, confidence) VALUES ('m1', 't1', 0.5)",
            [],
        )
        .unwrap();

        // Reproduce the corrupted state: media_tags carries an FK to `variants`,
        // which no longer exists.
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        conn.execute_batch(
            // A database in this state predates 0033, so forget that it ran.
            "DELETE FROM _migrations WHERE name = '0033_repair_dangling_variant_fks';
             CREATE TABLE variants (id TEXT PRIMARY KEY, media_id TEXT NOT NULL);
             DROP TABLE media_tags;
             CREATE TABLE media_tags (
                 media_id TEXT NOT NULL,
                 tag_id TEXT NOT NULL,
                 confidence REAL,
                 source TEXT,
                 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                 variant_id TEXT REFERENCES variants(id) ON DELETE CASCADE,
                 PRIMARY KEY (media_id, tag_id),
                 FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
                 FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
             );
             INSERT INTO media_tags (media_id, tag_id, confidence) VALUES ('m1', 't1', 0.5);
             DROP TABLE variants;",
        )
        .unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        // Sanity: the corruption reproduces the reported failure.
        assert!(table_ddl_mentions_variants(&conn, "media_tags"));
        assert!(
            conn.execute("DELETE FROM media_tags WHERE 0", []).is_err(),
            "corrupted DB should reject media_tags writes"
        );

        run_migrations(&mut conn).expect("repair");

        assert!(!table_has_column(&conn, "media_tags", "variant_id"));
        assert!(!table_ddl_mentions_variants(&conn, "media_tags"));
        conn.execute("DELETE FROM media_tags WHERE 0", [])
            .expect("media_tags must accept writes again after repair");

        let recorded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _migrations WHERE name = '0033_repair_dangling_variant_fks'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            recorded, 1,
            "0033 should be recorded after a successful repair"
        );

        // Row preservation.
        assert_eq!(count(&conn, "media"), 1);
        assert_eq!(count(&conn, "media_tags"), 1);
        assert_eq!(count(&conn, "tags"), 1);
        let confidence: f64 = conn
            .query_row(
                "SELECT confidence FROM media_tags WHERE media_id = 'm1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(confidence, 0.5);
    }

    #[test]
    fn test_is_file_for_media() {
        // The media's own file.
        assert!(is_file_for_media("01ABC.jpg", "01ABC"));
        assert!(is_file_for_media("01ABC", "01ABC"));

        // A derivative written by an older build as "{parent}_{child}".
        assert!(is_file_for_media("01PARENT_01ABC.png", "01ABC"));
        assert!(is_file_for_media("01PARENT_01ABC.webp", "01ABC"));

        // A parent does not own its derivative's file — that belongs to the child.
        assert!(!is_file_for_media("01PARENT_01ABC.png", "01PARENT"));

        // No prefix matching: neither a longer id nor a derivative of this id.
        assert!(!is_file_for_media("01ABCX.jpg", "01ABC"));
        assert!(!is_file_for_media("01ABC_01CHILD.png", "01ABC"));
    }

    /// Regression: deleting a derivative used to leave its file on disk, because
    /// the file was named after the parent and the old matcher only accepted
    /// names *starting with* the deleted id.
    #[test]
    fn test_delete_media_files_removes_own_and_derivative_files() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        for sub in ["library", "variants", "thumbnails"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }

        let id = "01CHILD";
        let parent = "01PARENT";
        let other = "01OTHER";
        let put =
            |sub: &str, name: String| std::fs::write(root.join(sub).join(name), b"x").unwrap();

        // Belong to `id` — must be removed.
        put("library", format!("{}.jpg", id));
        put("variants", format!("{}_{}.png", parent, id)); // old derivative naming
        put("thumbnails", format!("{}_256.jpg", id));
        put("thumbnails", format!("{}_512.jpg", id));

        // Belong to someone else — must survive.
        put("library", format!("{}.jpg", parent));
        put("library", format!("{}_{}.png", id, other)); // a derivative OF id
        put("thumbnails", format!("{}_256.jpg", other));

        delete_media_files(root, id);

        assert!(!root.join("library").join(format!("{}.jpg", id)).exists());
        assert!(!root
            .join("variants")
            .join(format!("{}_{}.png", parent, id))
            .exists());
        assert!(!root
            .join("thumbnails")
            .join(format!("{}_256.jpg", id))
            .exists());
        assert!(!root
            .join("thumbnails")
            .join(format!("{}_512.jpg", id))
            .exists());

        assert!(root
            .join("library")
            .join(format!("{}.jpg", parent))
            .exists());
        assert!(
            root.join("library")
                .join(format!("{}_{}.png", id, other))
                .exists(),
            "a derivative's file must survive its parent being deleted"
        );
        assert!(root
            .join("thumbnails")
            .join(format!("{}_256.jpg", other))
            .exists());
    }

    /// Temp app dir + migrated DB + one media row, for the resolver tests.
    /// Returns (keep-alive dir, connection, app_dir, db_path).
    fn resolver_env(
        id: &str,
        source_path: Option<&str>,
    ) -> (tempfile::TempDir, Connection, PathBuf, PathBuf) {
        let (dir, db_path) = new_test_db();
        let app_dir = dir.path().to_path_buf();
        let conn = open(&db_path);
        conn.execute(
            "INSERT INTO media (id, source_path, imported_at)
             VALUES (?1, ?2, '2026-01-01T00:00:00')",
            params![id, source_path],
        )
        .unwrap();
        (dir, conn, app_dir, db_path)
    }

    fn touch(app_dir: &Path, sub: &str, name: &str) -> PathBuf {
        let dir = app_dir.join(sub);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, b"x").unwrap();
        path
    }

    fn set_source_path(conn: &Connection, id: &str, path: &Path) {
        conn.execute(
            "UPDATE media SET source_path = ?1 WHERE id = ?2",
            params![path.to_string_lossy().to_string(), id],
        )
        .unwrap();
    }

    /// The reported bug: AI editing read the original import location instead of
    /// the library copy.
    #[test]
    fn test_resolve_prefers_library_copy_over_source_path() {
        let (dir, conn, app_dir, _db) = resolver_env("01M", None);
        let original = dir.path().join("original.jpg");
        std::fs::write(&original, b"orig").unwrap();
        set_source_path(&conn, "01M", &original);

        let in_library = touch(&app_dir, "library", "01M.jpg");

        assert_eq!(
            resolve_media_file_path(&app_dir, &conn, "01M").unwrap(),
            in_library
        );
    }

    #[test]
    fn test_resolve_finds_legacy_derivative_in_variants() {
        let (_dir, conn, app_dir, _db) = resolver_env("01CHILD", None);
        conn.execute(
            "INSERT INTO media (id, imported_at) VALUES ('01PARENT', '2026-01-01T00:00:00')",
            [],
        )
        .unwrap();

        let file = touch(&app_dir, "variants", "01PARENT_01CHILD.png");

        assert_eq!(
            resolve_media_file_path(&app_dir, &conn, "01CHILD").unwrap(),
            file
        );
        // The derivative's file belongs to the child, not to the parent.
        assert!(resolve_media_file_path(&app_dir, &conn, "01PARENT").is_err());
    }

    #[test]
    fn test_resolve_falls_back_to_source_path() {
        let (dir, conn, app_dir, _db) = resolver_env("01M", None);
        let original = dir.path().join("original.jpg");
        std::fs::write(&original, b"orig").unwrap();
        set_source_path(&conn, "01M", &original);

        std::fs::create_dir_all(app_dir.join("library")).unwrap();

        assert_eq!(
            resolve_media_file_path(&app_dir, &conn, "01M").unwrap(),
            original
        );
    }

    #[test]
    fn test_resolve_remote_url_without_local_copy_errors() {
        let (_dir, conn, app_dir, _db) = resolver_env("01M", Some("https://example.com/a.jpg"));
        std::fs::create_dir_all(app_dir.join("library")).unwrap();

        let err = resolve_media_file_path(&app_dir, &conn, "01M").unwrap_err();
        assert!(err.contains("网络链接"), "unexpected message: {}", err);
    }

    /// Guards the `starts_with` bug the old export resolver had.
    #[test]
    fn test_resolve_does_not_match_a_longer_id() {
        let (_dir, conn, app_dir, _db) = resolver_env("01ABC", None);
        touch(&app_dir, "library", "01ABCX.jpg");

        assert!(resolve_media_file_path(&app_dir, &conn, "01ABC").is_err());
    }

    #[test]
    fn test_resolve_ignores_derivatives_of_this_media() {
        let (_dir, conn, app_dir, _db) = resolver_env("01M", None);
        touch(&app_dir, "library", "01M_01OTHER.png");

        assert!(resolve_media_file_path(&app_dir, &conn, "01M").is_err());
    }

    /// Web-import shape: source_path is the URL, the bytes are in library/.
    #[test]
    fn test_resolve_library_wins_over_a_remote_source_path() {
        let (_dir, conn, app_dir, _db) = resolver_env("01M", Some("https://example.com/a.jpg"));
        let in_library = touch(&app_dir, "library", "01M.jpg");

        assert_eq!(
            resolve_media_file_path(&app_dir, &conn, "01M").unwrap(),
            in_library
        );
    }

    /// An exact `{id}.{ext}` must win over `{parent}_{id}.{ext}` regardless of
    /// the order `read_dir` happens to return them in.
    #[test]
    fn test_resolve_prefers_exact_stem_within_a_directory() {
        let (_dir, conn, app_dir, _db) = resolver_env("01M", None);
        touch(&app_dir, "library", "01PARENT_01M.jpg");
        let exact = touch(&app_dir, "library", "01M.jpg");

        assert_eq!(
            resolve_media_file_path(&app_dir, &conn, "01M").unwrap(),
            exact
        );
    }

    #[test]
    fn test_media_insert_and_list() {
        let (_dir, db_path) = new_test_db();

        // Insert via raw SQL
        let conn = open(&db_path);
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
             VALUES ('test_01', '/tmp/a.jpg', 800, 600, 50000, '2026-01-01T00:00:00', 'test', 'abc123')",
            [],
        )
        .unwrap();

        // Query via list_media_path
        let media = list_media_path(&db_path, "imported_at", true, 0, 100).unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].id, "test_01");
        assert_eq!(media[0].width, Some(800));
        assert_eq!(media[0].height, Some(600));
        assert_eq!(media[0].file_size, Some(50000));
    }

    #[test]
    fn test_tag_crud() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);

        // Create
        conn.execute("INSERT INTO tags (id, name) VALUES ('t1', 'landscape')", [])
            .unwrap();
        assert_eq!(
            conn.query_row::<String, _, _>(
                "SELECT name FROM tags WHERE id='t1'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            "landscape"
        );

        // Rename
        conn.execute("UPDATE tags SET name='scenery' WHERE id='t1'", [])
            .unwrap();
        assert_eq!(
            conn.query_row::<String, _, _>(
                "SELECT name FROM tags WHERE id='t1'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            "scenery"
        );

        // List via tag_list_path
        let tags = tag_list_path(&db_path).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "scenery");

        // Delete
        conn.execute("DELETE FROM tags WHERE id='t1'", []).unwrap();
        assert_eq!(count(&conn, "tags"), 0);
    }

    #[test]
    fn test_tag_add_remove_from_media() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);

        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at) VALUES ('m1', '/tmp/x.png', 100, 100, 1024, '2026-01-01T00:00:00')",
            [],
        ).unwrap();
        conn.execute("INSERT INTO tags (id, name) VALUES ('t1', 'sunset')", [])
            .unwrap();

        // Add tag to media
        conn.execute(
            "INSERT INTO media_tags (media_id, tag_id) VALUES ('m1', 't1')",
            [],
        )
        .unwrap();
        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM media_tags WHERE media_id='m1'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            1
        );

        // Remove
        conn.execute(
            "DELETE FROM media_tags WHERE media_id='m1' AND tag_id='t1'",
            [],
        )
        .unwrap();
        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM media_tags WHERE media_id='m1'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn test_collection_create_and_pin() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);

        // Create collection
        conn.execute(
            "INSERT INTO collections (id, name) VALUES ('c1', 'Favorites')",
            [],
        )
        .unwrap();

        let collections = conn
            .query_row::<i64, _, _>("SELECT COUNT(*) FROM collections", [], |r| r.get(0))
            .unwrap();
        assert_eq!(collections, 1);

        // Pin
        conn.execute(
            "UPDATE collections SET pinned_at='2026-01-01T00:00:00' WHERE id='c1'",
            [],
        )
        .unwrap();
        let pinned: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM collections WHERE pinned_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pinned, 1);

        // Unpin
        conn.execute("UPDATE collections SET pinned_at=NULL WHERE id='c1'", [])
            .unwrap();
        let unpinned: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM collections WHERE pinned_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(unpinned, 0);
    }

    #[test]
    fn test_collection_add_remove_items() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);

        conn.execute(
            "INSERT INTO collections (id, name) VALUES ('c1', 'Album')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at) VALUES ('m1', '/tmp/1.jpg', 100, 100, 500, '2026-01-01T00:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at) VALUES ('m2', '/tmp/2.jpg', 200, 200, 500, '2026-01-01T00:00:00')",
            [],
        ).unwrap();

        // Add items
        conn.execute(
            "INSERT INTO collection_items (collection_id, media_id) VALUES ('c1', 'm1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO collection_items (collection_id, media_id) VALUES ('c1', 'm2')",
            [],
        )
        .unwrap();
        assert_eq!(count(&conn, "collection_items"), 2);
        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM collection_items WHERE collection_id='c1'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            2
        );

        // Remove one
        conn.execute(
            "DELETE FROM collection_items WHERE collection_id='c1' AND media_id='m1'",
            [],
        )
        .unwrap();
        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM collection_items WHERE collection_id='c1'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            1
        );
    }

    #[test]
    fn test_fk_cascade_delete_media() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);
        let now = "2026-01-01T00:00:00";

        // Create media with all dependent records
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
             VALUES ('del_test', '/tmp/del.png', 100, 100, 1024, ?1, 'test', 'deadbeef')",
            params![now],
        )
        .unwrap();
        conn.execute("INSERT INTO tags (id, name) VALUES ('dt', 'temp')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO media_tags (media_id, tag_id) VALUES ('del_test', 'dt')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO captions (id, media_id, text, created_at, updated_at) VALUES ('cap_del', 'del_test', 'hello', ?1, ?1)",
            params![now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO embeddings (media_id, content_type, model, vector) VALUES ('del_test', 'caption', 'test', X'0000803F')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO collections (id, name) VALUES ('col_del', 'Test')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO collection_items (collection_id, media_id) VALUES ('col_del', 'del_test')",
            [],
        )
        .unwrap();

        // Verify all exist
        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM captions WHERE media_id='del_test'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM media_tags WHERE media_id='del_test'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            1
        );

        // Delete media — cascade should clean up
        conn.execute("DELETE FROM media WHERE id='del_test'", [])
            .unwrap();

        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM captions WHERE media_id='del_test'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            0,
            "captions should cascade"
        );
        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM embeddings WHERE media_id='del_test'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            0,
            "embeddings should cascade"
        );
        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM media_tags WHERE media_id='del_test'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            0,
            "media_tags should cascade"
        );
        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM collection_items WHERE media_id='del_test'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            0,
            "collection_items should cascade"
        );

        // Clean up reference data
        conn.execute("DELETE FROM tags WHERE id='dt'", []).unwrap();
        conn.execute("DELETE FROM collections WHERE id='col_del'", [])
            .unwrap();
    }

    #[test]
    fn test_soft_delete_and_recover() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);

        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at) VALUES ('sd1', '/tmp/sd.jpg', 100, 100, 1024, '2026-01-01T00:00:00')",
            [],
        ).unwrap();

        // Soft delete
        conn.execute(
            "UPDATE media SET deleted_at='2026-06-01T00:00:00' WHERE id='sd1'",
            [],
        )
        .unwrap();
        let in_trash: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM media WHERE id='sd1' AND deleted_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(in_trash, 1);

        // Recover
        conn.execute("UPDATE media SET deleted_at=NULL WHERE id='sd1'", [])
            .unwrap();
        let recovered: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM media WHERE id='sd1' AND deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(recovered, 1);
    }

    #[test]
    fn test_settings_crud() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);

        // Insert
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('test_theme', 'dark')",
            [],
        )
        .unwrap();
        let val: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key='test_theme'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(val, "dark");

        // Update
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('test_theme', 'light')",
            [],
        )
        .unwrap();
        let val: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key='test_theme'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(val, "light");

        // Delete
        conn.execute("DELETE FROM settings WHERE key='test_theme'", [])
            .unwrap();
        let gone: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key='test_theme'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(gone, 0);
    }

    #[test]
    fn test_all_core_tables_created() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);

        let expected = [
            "_migrations",
            "media",
            "tags",
            "media_tags",
            "collections",
            "collection_items",
            "captions",
            "embeddings",
            "settings",
            "media_lineage",
            "comfyui_workflows",
        ];

        for table in &expected {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    params![table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "table '{}' should exist", table);
        }
    }

    #[test]
    fn test_find_items_with_tags() {
        let (_dir, db_path) = new_test_db();
        let conn = open(&db_path);

        // Insert test data
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at) VALUES ('ft1', '/tmp/ft1.jpg', 100, 100, 100, '2026-01-01T00:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at) VALUES ('ft2', '/tmp/ft2.jpg', 200, 200, 200, '2026-01-01T00:00:00')",
            [],
        ).unwrap();
        conn.execute("INSERT INTO tags (id, name) VALUES ('tag_a', 'alpha')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (id, name) VALUES ('tag_b', 'beta')", [])
            .unwrap();

        // ft1 has both tags, ft2 has only tag_a
        conn.execute(
            "INSERT INTO media_tags (media_id, tag_id) VALUES ('ft1', 'tag_a')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO media_tags (media_id, tag_id) VALUES ('ft1', 'tag_b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO media_tags (media_id, tag_id) VALUES ('ft2', 'tag_a')",
            [],
        )
        .unwrap();

        // Build minimal browse items
        fn make_item(id: &str, w: i32, h: i32, sz: i64) -> BrowseItem {
            BrowseItem {
                id: id.into(),
                media_id: id.into(),
                source_path: Some(format!("/tmp/{}.jpg", id)),
                width: Some(w),
                height: Some(h),
                file_size: Some(sz),
                created_at: None,
                modified_at: None,
                imported_at: "2026-01-01T00:00:00".into(),
                source_url: None,
                page_url: None,
                source: None,
                sha256: None,
                deleted_at: None,
                thumb_256: None,
                lqip: None,
                media_type: Some("image".into()),
                duration: None,
                video_codec: None,
                video_fps: None,
                child_count: 0,
                parent_count: 0,
            }
        }
        let items = vec![
            make_item("ft1", 100, 100, 100),
            make_item("ft2", 200, 200, 200),
        ];

        // Union (any tag) — both media have alpha, so both match
        let matching =
            find_items_with_tags_path(&db_path, &items, &["alpha".into(), "beta".into()]).unwrap();
        assert_eq!(
            matching.len(),
            2,
            "union of alpha+beta should match both media"
        );
        assert!(matching.contains("ft1"));
        assert!(matching.contains("ft2"));

        // Single tag — both should match
        let matching = find_items_with_tags_path(&db_path, &items, &["alpha".into()]).unwrap();
        assert_eq!(matching.len(), 2);
        assert!(matching.contains("ft1"));
        assert!(matching.contains("ft2"));

        // Non-existent tag — no matches
        let matching = find_items_with_tags_path(&db_path, &items, &["nope".into()]).unwrap();
        assert_eq!(matching.len(), 0);
    }
}
