# Variant System Refactor: Unified Media + Lineage Chain

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `variants` table with a unified `media` + `media_lineage` model where all images are first-class media records linked by a DAG inheritance chain.

**Architecture:** Dismantle the original/variant distinction. Every image — imported, AI-generated, or AI-edited — becomes a `media` row. Derivative relationships (A edited to produce B) are stored in a `media_lineage` junction table supporting multiple parents per child. The browse view switches from UNION ALL flattening to a simple media query with an optional root-node filter.

**Tech Stack:** Rust (Tauri v2), SQLite, React 19 + TypeScript + Tailwind CSS

**Spec:** `docs/superpowers/specs/2026-07-02-variant-lineage-refactor-design.md`

---

### Task 1: Add media_lineage table and media.source column

**Files:**
- Modify: `src-tauri/src/db/mod.rs` — migration functions

- [ ] **Step 1: Add migration 00025 — create media_lineage table**

```rust
// In run_migrations(), after the 00024 guard:

// 0025: media_lineage table for DAG inheritance
{
    let version: String = db
        .query_row(
            "SELECT version FROM _migrations WHERE version = '0025'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    if version.is_empty() {
        db.execute_batch(
            "
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
            CREATE INDEX IF NOT EXISTS idx_lineage_child ON media_lineage(child_media_id);
            INSERT OR IGNORE INTO _migrations (version) VALUES ('0025');
            ",
        )
        .expect("migration 0025: media_lineage table");
    }
}
```

- [ ] **Step 2: Add migration 00026 — add source column to media**

```rust
// 0026: add source column to media table
{
    let version: String = db
        .query_row(
            "SELECT version FROM _migrations WHERE version = '0026'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    if version.is_empty() {
        db.execute_batch(
            "
            ALTER TABLE media ADD COLUMN source TEXT;
            INSERT OR IGNORE INTO _migrations (version) VALUES ('0026');
            ",
        )
        .expect("migration 0026: media.source column");
    }
}
```

- [ ] **Step 3: Write migration 00027 — migrate each variant to a media record + lineage link**

```rust
// 0027: migrate variants to media + media_lineage
{
    let version: String = db
        .query_row(
            "SELECT version FROM _migrations WHERE version = '0027'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    if version.is_empty() {
        // Insert variants as media records, reusing variant id
        db.execute_batch(
            "
            INSERT OR IGNORE INTO media (
                id, file_path, file_name, file_size, format,
                width, height, media_type, source, created_at
            )
            SELECT
                v.id,
                v.file_path,
                REPLACE(
                    REPLACE(v.file_path, RTRIM(v.file_path, REPLACE(v.file_path, '/', '')), ''),
                    RTRIM(RTRIM(v.file_path, REPLACE(v.file_path, '/', '')), '/'),
                    ''
                ),
                v.file_size,
                v.format,
                v.width,
                v.height,
                COALESCE(v.media_type, 'image'),
                v.source,
                v.created_at
            FROM variants v
            WHERE NOT EXISTS (SELECT 1 FROM media WHERE id = v.id);

            -- Build lineage links: old variant → new media, with original as parent
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

            -- Reassign captions from variant_id to the new media id
            UPDATE captions SET media_id = variant_id
            WHERE variant_id IS NOT NULL
              AND EXISTS (SELECT 1 FROM media WHERE id = variant_id);

            -- Reassign embeddings from variant_id to the new media id
            UPDATE embeddings SET media_id = variant_id
            WHERE variant_id IS NOT NULL
              AND EXISTS (SELECT 1 FROM media WHERE id = variant_id);

            -- Reassign media_tags from variant_id to the new media id
            UPDATE media_tags SET media_id = variant_id
            WHERE variant_id IS NOT NULL
              AND EXISTS (SELECT 1 FROM media WHERE id = variant_id);

            INSERT OR IGNORE INTO _migrations (version) VALUES ('0027');
            ",
        )
        .expect("migration 0027: variant → media migration");
    }
}
```

- [ ] **Step 4: Write migration 00028 — clean up old columns and tables**

```rust
// 0028: drop old variant-related columns and tables
{
    let version: String = db
        .query_row(
            "SELECT version FROM _migrations WHERE version = '0028'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    if version.is_empty() {
        db.execute_batch(
            "
            -- Drop display_variant_id from media
            ALTER TABLE media DROP COLUMN display_variant_id;

            -- Drop variant_id columns from child tables
            ALTER TABLE captions DROP COLUMN variant_id;
            ALTER TABLE embeddings DROP COLUMN variant_id;
            ALTER TABLE media_tags DROP COLUMN variant_id;

            -- Drop old variants table and variant_presets table
            DROP TABLE IF EXISTS variants;
            DROP TABLE IF EXISTS variant_presets;

            INSERT OR IGNORE INTO _migrations (version) VALUES ('0028');
            ",
        )
        .expect("migration 0028: cleanup old columns and tables");
    }
}
```

- [ ] **Step 5: Run migration verification**

Run: `cargo build`
Expected: compiles without errors from the new migration code.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/db/mod.rs
git commit -m "feat: add media_lineage table and data migration (00025-00028)"
```

---

### Task 2: Add media_lineage database CRUD functions

**Files:**
- Modify: `src-tauri/src/db/mod.rs` — add lineage_* functions after the variant functions

- [ ] **Step 1: Add lineage_insert function**

```rust
pub fn lineage_insert_path(
    db: &Connection,
    parent_media_id: &str,
    child_media_id: &str,
    relation_type: &str,
    workflow_id: Option<&str>,
) -> Result<(), rusqlite::Error> {
    db.execute(
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
    let db = get_db(app)?;
    lineage_insert_path(&db, parent_media_id, child_media_id, relation_type, workflow_id)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Add lineage_list function — returns parents and children for a media item**

```rust
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

pub fn lineage_list_path(db: &Connection, media_id: &str) -> Result<LineageGraph, rusqlite::Error> {
    let mut stmt = db.prepare(
        "SELECT parent_media_id, relation_type, workflow_id, created_at
         FROM media_lineage WHERE child_media_id = ?1 ORDER BY created_at"
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

    let mut stmt = db.prepare(
        "SELECT child_media_id, relation_type, workflow_id, created_at
         FROM media_lineage WHERE parent_media_id = ?1 ORDER BY created_at"
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
    let db = get_db(app)?;
    lineage_list_path(&db, media_id).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Add lineage_remove and cycle detection function**

```rust
pub fn lineage_remove_path(
    db: &Connection,
    parent_media_id: &str,
    child_media_id: &str,
) -> Result<(), rusqlite::Error> {
    db.execute(
        "DELETE FROM media_lineage WHERE parent_media_id = ?1 AND child_media_id = ?2",
        params![parent_media_id, child_media_id],
    )?;
    Ok(())
}

/// Check if inserting (parent → child) would create a cycle in the DAG.
/// Returns true if a cycle would be formed.
pub fn lineage_would_cycle_path(db: &Connection, parent_id: &str, child_id: &str) -> Result<bool, rusqlite::Error> {
    use std::collections::HashSet;
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = vec![child_id.to_string()];

    while let Some(current) = queue.pop() {
        if current == parent_id {
            return Ok(true);
        }
        if visited.insert(current.clone()) {
            let mut stmt = db.prepare(
                "SELECT child_media_id FROM media_lineage WHERE parent_media_id = ?1"
            )?;
            let children: Vec<String> = stmt
                .query_map(params![current], |r| r.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            queue.extend(children);
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
    let db = get_db(app)?;
    if lineage_would_cycle_path(&db, parent_media_id, child_media_id).map_err(|e| e.to_string())? {
        return Err("Adding this lineage would create a cycle".into());
    }
    lineage_insert_path(&db, parent_media_id, child_media_id, relation_type, workflow_id)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Add lineage_roots_query — get root media (no parents)**

```rust
pub fn media_is_root(db: &Connection, media_id: &str) -> Result<bool, rusqlite::Error> {
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM media_lineage WHERE child_media_id = ?1",
        params![media_id],
        |r| r.get(0),
    )?;
    Ok(count == 0)
}

pub fn media_has_derivatives(db: &Connection, media_id: &str) -> Result<bool, rusqlite::Error> {
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM media_lineage WHERE parent_media_id = ?1",
        params![media_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/mod.rs
git commit -m "feat: add media_lineage CRUD and cycle detection functions"
```

---

### Task 3: Update browse query — remove variant UNION ALL, add root filter

**Files:**
- Modify: `src-tauri/src/db/mod.rs` — `list_browse_items_path` and `browse_query_filtered_path`
- Modify: `src-tauri/src/media/mod.rs` — `BrowseItem` struct and `VariantVisibility`

- [ ] **Step 1: Simplify BrowseItem struct**

In `src-tauri/src/media/mod.rs`, replace the existing `BrowseItem` struct:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct BrowseItem {
    pub id: String,
    pub media_id: String,
    pub source_path: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub file_size: Option<i64>,
    pub created_at: Option<String>,
    pub imported_at: Option<String>,
    pub source_url: Option<String>,
    pub page_url: Option<String>,
    pub source: Option<String>,
    pub sha256: Option<String>,
    pub thumb_256: Option<String>,
    pub lqip: Option<String>,
    pub media_type: Option<String>,
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub video_fps: Option<f64>,
    pub has_derivatives: bool,
    pub parent_count: i32,
}
```

Remove `item_kind`, `variant_id`, `is_display_variant`. Add `has_derivatives` and `parent_count`.

- [ ] **Step 2: Rewrite list_browse_items_path — single media query with computed fields**

In `src-tauri/src/db/mod.rs`, replace the UNION ALL query in `list_browse_items_path`:

```rust
// Replace the existing UNION ALL query (lines ~963-1012) with:
let query = format!(
    "SELECT
        m.id,
        m.source_path,
        m.width,
        m.height,
        m.file_size,
        m.created_at,
        m.imported_at,
        m.source_url,
        m.page_url,
        m.source,
        m.sha256,
        m.thumb_256,
        m.lqip,
        m.media_type,
        m.duration,
        m.video_codec,
        m.video_fps,
        CASE WHEN EXISTS (
            SELECT 1 FROM media_lineage WHERE parent_media_id = m.id
        ) THEN 1 ELSE 0 END AS has_derivatives,
        (SELECT COUNT(*) FROM media_lineage WHERE child_media_id = m.id) AS parent_count
    FROM media m
    WHERE m.deleted_at IS NULL
    {}
    ORDER BY m.imported_at DESC
    {}",
    root_filter,
    pagination
);
```

The `root_filter` is:
- Representative mode: `AND m.id NOT IN (SELECT child_media_id FROM media_lineage)` (roots only)
- All mode: empty string

```rust
let root_filter = match visibility {
    VariantVisibility::Representative => {
        "AND m.id NOT IN (SELECT child_media_id FROM media_lineage)"
    }
    VariantVisibility::All => "",
};
```

Remove the `item_kind` ordering from the ORDER BY clause.

- [ ] **Step 3: Update BrowseItem mapping to match new query**

In the same function, update the row mapping to match the new column list:

```rust
let items = stmt
    .query_map(params![], |row| {
        Ok(BrowseItem {
            id: row.get(0)?,
            media_id: row.get(0)?, // id and media_id are now the same
            source_path: row.get(1)?,
            width: row.get(2)?,
            height: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
            imported_at: row.get(6)?,
            source_url: row.get(7)?,
            page_url: row.get(8)?,
            source: row.get(9)?,
            sha256: row.get(10)?,
            thumb_256: row.get(11)?,
            lqip: row.get(12)?,
            media_type: row.get(13)?,
            duration: row.get(14)?,
            video_codec: row.get(15)?,
            video_fps: row.get(16)?,
            has_derivatives: row.get::<_, i32>(17)? != 0,
            parent_count: row.get(18)?,
        })
    })?
    .filter_map(|r| r.ok())
    .collect();
```

The `media_id` is now always the same as `id` (both are `m.id`).

- [ ] **Step 4: Update browse_query_filtered_path similarly**

Apply the same changes to `browse_query_filtered_path` (the version with `WHERE m.id IN ({})`). Merge both versions' filter logic.

- [ ] **Step 5: Rename VariantVisibility to BrowseVisibility**

In `src-tauri/src/media/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrowseVisibility {
    Representative,  // only root nodes (no parents)
    All,             // all media including derivatives
}
```

- [ ] **Step 6: Build and verify**

Run: `cargo build`
Expected: compiles. Fix any type errors in BrowseItem construction.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/mod.rs src-tauri/src/media/mod.rs
git commit -m "refactor: simplify browse query — remove variant UNION ALL, add lineage root filter"
```

---

### Task 4: Remove variant commands, add lineage commands

**Files:**
- Create: `src-tauri/src/commands/lineage.rs`
- Delete: `src-tauri/src/commands/variant.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Create lineage commands file**

```rust
// src-tauri/src/commands/lineage.rs
use tauri::AppHandle;
use crate::db;

#[tauri::command]
pub async fn media_lineage_list(app: AppHandle, media_id: String) -> Result<db::LineageGraph, String> {
    db::lineage_list(&app, &media_id)
}

#[tauri::command]
pub async fn media_lineage_add(
    app: AppHandle,
    parent_id: String,
    child_id: String,
    relation_type: String,
) -> Result<(), String> {
    db::lineage_insert_safe(&app, &parent_id, &child_id, &relation_type, None)
}

#[tauri::command]
pub async fn media_lineage_remove(
    app: AppHandle,
    parent_id: String,
    child_id: String,
) -> Result<(), String> {
    let db = db::get_db(&app)?;
    db::lineage_remove_path(&db, &parent_id, &child_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn media_list_roots_count(app: AppHandle) -> Result<u64, String> {
    let db = db::get_db(&app)?;
    let count: u64 = db
        .query_row(
            "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL AND id NOT IN (SELECT child_media_id FROM media_lineage)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count)
}
```

- [ ] **Step 2: Update commands/mod.rs**

Replace `mod variant;` with `mod lineage;`, and `pub use variant::*;` with `pub use lineage::*;`:

```rust
// In mod.rs, replace:
// mod variant;
// pub use variant::*;
//
// With:
mod lineage;
pub use lineage::*;
```

- [ ] **Step 3: Update main.rs — replace command registrations**

In `src-tauri/src/main.rs`:

Remove from `use` block:
```
variant_annotate, variant_delete, variant_generate, variant_import, variant_list,
variant_preset_create, variant_preset_delete, variant_presets,
media_reset_all_display_variants, media_set_display_variant,
```

Add to `use` block:
```
media_lineage_list, media_lineage_add, media_lineage_remove, media_list_roots_count,
```

In the invoke_handler, replace all variant command registrations with the new lineage commands:

```rust
// Remove:
// variant_annotate,
// variant_delete,
// variant_generate,
// variant_import,
// variant_list,
// variant_preset_create,
// variant_preset_delete,
// variant_presets,
// media_reset_all_display_variants,
// media_set_display_variant,

// Add:
media_lineage_list,
media_lineage_add,
media_lineage_remove,
media_list_roots_count,
```

Also remove `mod variants;` from the module declarations at the top of main.rs.

- [ ] **Step 4: Delete variants directory**

Delete `src-tauri/src/variants/` directory (contains `mod.rs`).

Remove `pub mod variants;` from `src-tauri/src/main.rs` if it exists there (it's declared at the top).

- [ ] **Step 5: Build**

Run: `cargo build`
Expected: compiles successfully.

- [ ] **Step 6: Commit**

```bash
git rm src-tauri/src/variants/mod.rs src-tauri/src/commands/variant.rs
git add src-tauri/src/commands/lineage.rs src-tauri/src/commands/mod.rs src-tauri/src/main.rs
git commit -m "refactor: replace variant commands with lineage commands"
```

---

### Task 5: Update AI queue — Vec<String> source_media_ids, new import logic

**Files:**
- Modify: `src-tauri/src/ai/imagine/queue.rs`

- [ ] **Step 1: Change ImageTask::Edit to use source_media_ids**

```rust
pub enum ImageTask {
    Generate {
        task_id: String,
        prompt: String,
        aspect_ratio: String,
        resolution: String,
        n: u32,
        workflow_id: Option<String>,
    },
    Edit {
        task_id: String,
        source_media_ids: Vec<String>,  // changed from media_id + variant_id
        prompt: String,
        aspect_ratio: String,
        resolution: String,
        n: u32,
        workflow_id: Option<String>,
    },
}
```

Also update `TaskState` — change `media_id: Option<String>` and remove `variant_id`:

```rust
struct TaskState {
    // ...
    source_media_ids: Option<Vec<String>>,
    // ...
}
```

- [ ] **Step 2: Update image_queue_submit_edit signature**

```rust
pub async fn image_queue_submit_edit(
    app: AppHandle,
    source_media_ids: Vec<String>,
    prompt: String,
    aspect_ratio: String,
    resolution: String,
    n: u32,
    workflow_id: Option<String>,
) -> Result<String, String> {
    let queue = get_queue(&app)?;
    let task_id = Ulid::new().to_string();

    let task_state = TaskState {
        task_id: task_id.clone(),
        task_type: "edit".into(),
        source_media_ids: Some(source_media_ids.clone()),
        prompt: Some(prompt.clone()),
        status: "pending".into(),
        staged: vec![],
        created_at: Utc::now().to_rfc3339(),
        workflow_id: workflow_id.clone(),
    };
    // ... rest stays the same

    let task = ImageTask::Edit {
        task_id: task_id.clone(),
        source_media_ids,
        prompt,
        aspect_ratio,
        resolution,
        n,
        workflow_id,
    };
    // ...
}
```

- [ ] **Step 3: Update process_task edit path — resolve multiple images**

```rust
fn resolve_edit_images(
    app: &AppHandle,
    source_media_ids: &[String],
    max_dim: u32,
) -> Result<Vec<EditSourceImage>, String> {
    let mut images = Vec::new();
    for mid in source_media_ids {
        let path = find_media_path(app, mid)?;
        let bytes = std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?;
        let img = image::load_from_memory(&bytes).map_err(|e| format!("decode {path}: {e}"))?;
        let (w, h) = if img.width() > max_dim || img.height() > max_dim {
            let resized = img.resize(max_dim, max_dim, image::imageops::FilterType::Lanczos3);
            (resized.width(), resized.height())
        } else {
            (img.width(), img.height())
        };
        // Encode to base64 data URL
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cursor, image::ImageFormat::Jpeg)
            .map_err(|e| e.to_string())?;
        let data_url = format!("data:image/jpeg;base64,{}", BASE64_STANDARD.encode(&buf));
        images.push(EditSourceImage {
            media_id: mid.clone(),
            data_url,
            width: w as u32,
            height: h as u32,
        });
    }
    Ok(images)
}
```

In the process_task Edit arm:

```rust
ImageTask::Edit { ref source_media_ids, ref prompt, ref aspect_ratio, ref resolution, n, .. } => {
    let sources = resolve_edit_images(&app, source_media_ids, max_dim as u32)?;
    // For single-image providers (xAI): use first source
    // For multi-image providers (ComfyUI): use all sources
    let params = if sources.len() == 1 {
        EditParams {
            prompt: prompt.clone(),
            image_data_url: sources[0].data_url.clone(),
            aspect_ratio: aspect_ratio.clone(),
            resolution: resolution.clone(),
            n,
        }
    } else {
        // Multi-image: pass all data URLs (provider decides how to use them)
        EditParams {
            prompt: prompt.clone(),
            image_data_url: sources[0].data_url.clone(), // primary
            aspect_ratio: aspect_ratio.clone(),
            resolution: resolution.clone(),
            n,
        }
    };
    provider.edit(&params).await
}
```

- [ ] **Step 4: Update image_queue_import — create media records + lineage instead of variants**

In the `image_queue_import` function, replace the edit-mode variant creation (lines ~532-601) with:

```rust
if let Some(ref source_ids) = task.source_media_ids {
    // Edit mode: each staged image becomes a derivative media record
    let provider = task.workflow_id.as_deref().map_or("unknown", |_| {
        if task.workflow_id.as_deref() == Some("comfyui") { "comfyui" } else { "xai" }
    });

    for staged in &selected {
        if let Some(info) = task.staged.iter().find(|s| s.id == *staged) {
            let new_id = Ulid::new().to_string();
            let ext = info.path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg");
            let dest = variants_dir.join(format!("{}_{}.{}", source_ids.first().unwrap_or(&new_id), new_id, ext));

            std::fs::copy(&info.path, &dest).map_err(|e| e.to_string())?;
            std::fs::remove_file(&info.path).ok();

            let dims = image::ImageReader::open(&dest)
                .map_err(|e| e.to_string())?
                .into_dimensions()
                .unwrap_or((0, 0));

            let file_size = std::fs::metadata(&dest).map(|m| m.len() as i64).unwrap_or(0);
            let dest_str = dest.to_string_lossy().replace('\\', "/");

            // Insert as a new media record (not a variant)
            db::media_insert_path(
                &db,
                &new_id,
                &dest_str,
                "",
                file_size,
                ext,
                Some(dims.0 as i32),
                Some(dims.1 as i32),
                Some("image"),
                Some(&format!("ai-edited:{}", provider)),
            )
            .map_err(|e| e.to_string())?;

            // Create lineage links for all source images
            for src_id in source_ids {
                db::lineage_insert_path(&db, src_id, &new_id, "edit", task.workflow_id.as_deref())
                    .map_err(|e| e.to_string())?;
            }

            // Save prompt as caption if not empty
            if !task.prompt.as_ref().map_or(true, |p| p.is_empty()) {
                let prompt = task.prompt.as_deref().unwrap_or("");
                db::caption_create_path(
                    &db,
                    &new_id,
                    None,
                    prompt,
                    &format!("ai-edit"),
                )
                .map_err(|e| e.to_string())?;
            }

            // Generate thumbnail
            generate_variant_thumbnail_path(&dest_str, &new_id, &app_data_dir)
                .map_err(|e| e.to_string())?;

            // Enqueue AI annotation
            // ... (same as before but with new media_id)
        }
    }
}
```

- [ ] **Step 5: Update resolve_edit_image to find_media_path — remove variant logic**

Replace the old `resolve_edit_image` (which checks variant_id) with a simpler `find_media_path`:

```rust
fn find_media_path(app: &AppHandle, media_id: &str) -> Result<String, String> {
    let db = crate::db::get_db(app)?;
    let path: String = db
        .query_row(
            "SELECT file_path FROM media WHERE id = ?1",
            params![media_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("media not found: {e}"))?;
    Ok(path)
}
```

- [ ] **Step 6: Build**

Run: `cargo build`
Expected: compiles after fixing any type mismatches.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/ai/imagine/queue.rs
git commit -m "refactor: update AI queue for multi-source media lineage model"
```

---

### Task 6: Update ComfyUI workflow — multi-image support

**Files:**
- Modify: `src-tauri/src/ai/imagine/workflow.rs`
- Modify: `src-tauri/src/ai/imagine/comfyui.rs`

- [ ] **Step 1: Update input_key_for_param for numbered input_image**

```rust
fn input_key_for_param(param_name: &str) -> &str {
    match param_name {
        "prompt" | "positive_prompt" => "text",
        "negative_prompt" => "text",
        _ if param_name.starts_with("input_image") => "image",
        _ => param_name,
    }
}
```

This handles `#input_image_1`, `#input_image_2`, etc., mapping them all to the "image" input key. The specific node is identified by node_id, so multiple input_image params across different nodes work correctly.

- [ ] **Step 2: Update ComfyuiProvider::edit() to upload multiple images**

```rust
async fn edit(&self, params: &EditParams) -> Result<Vec<GeneratedImage>, ImagineError> {
    // Extract base64 images from params
    // For now: single image from image_data_url
    let (mime, b64) = params.image_data_url
        .split_once(";base64,")
        .ok_or(ImagineError::Provider("invalid data URL".into()))?;
    let data = BASE64_STANDARD.decode(b64)
        .map_err(|e| ImagineError::Provider(format!("base64 decode: {e}")))?;

    let ext = if mime.contains("png") { "png" } else { "jpg" };
    let part = reqwest::multipart::Part::bytes(data)
        .file_name(format!("input.{}", ext))
        .mime_str(mime)
        .map_err(|e| ImagineError::Provider(format!("mime: {e}")))?;

    let form = reqwest::multipart::Form::new().part("image", part);
    let resp = self.client
        .post(format!("{}/upload/image", self.base_url))
        .multipart(form)
        .send()
        .await
        .map_err(|e| ImagineError::Provider(format!("upload: {e}")))?;

    let upload_json: serde_json::Value = resp.json().await
        .map_err(|e| ImagineError::Provider(format!("upload response: {e}")))?;
    let filename = upload_json["name"].as_str()
        .ok_or(ImagineError::Provider("no filename in upload response".into()))?;

    let mut values: HashMap<String, String> = HashMap::new();
    if !params.prompt.is_empty() {
        values.insert("prompt".into(), params.prompt.clone());
    }
    values.insert("input_image".into(), filename.to_string());

    // For future: support multiple image uploads via params.additional_images
    // Each additional image would be uploaded and injected as input_image_2, input_image_3, etc.

    self.submit_and_wait(&values).await
}
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ai/imagine/workflow.rs src-tauri/src/ai/imagine/comfyui.rs
git commit -m "feat: support numbered input_image params for multi-source workflows"
```

---

### Task 7: Update TypeScript types

**Files:**
- Create: `src/types/lineage.ts`
- Delete: `src/types/variant.ts`
- Modify: `src/types/media.ts`

- [ ] **Step 1: Create lineage types**

```typescript
// src/types/lineage.ts
export interface LineageEdge {
  media_id: string;
  relation_type: string;
  workflow_id: string | null;
  created_at: string;
}

export interface LineageGraph {
  parents: LineageEdge[];
  children: LineageEdge[];
}
```

- [ ] **Step 2: Update Media interface**

In `src/types/media.ts`, add `source` and `has_derivatives` / `parent_count` to Media:

```typescript
export interface Media {
  id: string;
  source_path: string;
  width: number | null;
  height: number | null;
  file_size: number | null;
  created_at: string | null;
  modified_at: string | null;
  imported_at: string | null;
  source_url: string | null;
  page_url: string | null;
  source: string | null;
  sha256: string | null;
  deleted_at: string | null;
  thumb_256: string | null;
  lqip: string | null;
  media_type: string | null;
  duration: number | null;
  video_codec: string | null;
  video_fps: number | null;
  // New:
  has_derivatives?: boolean;
  parent_count?: number;
}
```

Remove `display_variant_id` from the interface.

- [ ] **Step 3: Delete variant.ts**

```bash
git rm src/types/variant.ts
```

- [ ] **Step 4: Check for TypeScript import errors**

Run: `npx tsc --noEmit`
Expected: errors about missing `variant.ts` imports. Note which files import from `variant.ts` for Task 8.

- [ ] **Step 5: Commit**

```bash
git add src/types/lineage.ts src/types/media.ts
git rm src/types/variant.ts
git commit -m "refactor: update TypeScript types — remove Variant, add LineageGraph"
```

---

### Task 8: Update Tauri API wrappers (tauri.ts)

**Files:**
- Modify: `src/lib/tauri.ts`

- [ ] **Step 1: Remove variant wrapper functions**

Remove these functions (lines ~263-324):
- `variantList`
- `variantGenerate`
- `variantImport`
- `variantDelete`
- `variantPresets`
- `variantPresetCreate`
- `variantPresetDelete`
- `variantAnnotate`
- `mediaSetDisplayVariant`
- `mediaResetAllDisplayVariants`

- [ ] **Step 2: Add lineage wrapper functions**

```typescript
// Add after the imagine functions:
export async function mediaLineageList(mediaId: string): Promise<LineageGraph> {
  return invoke("media_lineage_list", { mediaId });
}

export async function mediaLineageAdd(
  parentId: string,
  childId: string,
  relationType: string,
): Promise<void> {
  return invoke("media_lineage_add", { parentId, childId, relationType });
}

export async function mediaLineageRemove(
  parentId: string,
  childId: string,
): Promise<void> {
  return invoke("media_lineage_remove", { parentId, childId });
}

export async function mediaListRootsCount(): Promise<number> {
  return invoke("media_list_roots_count");
}
```

Add the import: `import type { LineageGraph } from "@/types/lineage";`

- [ ] **Step 3: Update imagine wrapper functions**

Change `imageQueueSubmitEdit` to use `sourceMediaIds`:

```typescript
export async function imageQueueSubmitEdit(
  sourceMediaIds: string[],
  prompt: string,
  aspectRatio?: string,
  resolution?: string,
  n?: number,
  workflowId?: string | null,
): Promise<string> {
  return invoke("image_queue_submit_edit", {
    sourceMediaIds,
    prompt,
    aspectRatio: aspectRatio || "auto",
    resolution: resolution || "1k",
    n: n || 1,
    workflowId: workflowId || null,
  });
}
```

Remove old `imageEdit`, `imageConfirmImport` functions (legacy non-queue API).

- [ ] **Step 4: Commit**

```bash
git add src/lib/tauri.ts
git commit -m "refactor: update tauri.ts wrappers — lineage commands, updated edit signature"
```

---

### Task 9: Update DetailPanel — remove variant UI, add lineage chain panel

**Files:**
- Modify: `src/components/DetailPanel/DetailPanel.tsx`

- [ ] **Step 1: Remove variant-related imports and state**

Remove imports:
```typescript
// Remove:
import { variantGenerate, variantImport, variantDelete, variantList, variantPresets, variantPresetCreate, variantPresetDelete, variantAnnotate, mediaSetDisplayVariant } from "@/lib/tauri";
import type { Variant, VariantPreset } from "@/types/variant";
```

Add imports:
```typescript
import { mediaLineageList } from "@/lib/tauri";
import type { LineageGraph } from "@/types/lineage";
```

- [ ] **Step 2: Replace variant state with lineage state**

```typescript
// Remove:
// const [variants, setVariants] = useState<Variant[]>([]);
// const [targetId, setTargetId] = useState<string | null>(null);

// Add:
const [lineage, setLineage] = useState<LineageGraph>({ parents: [], children: [] });
```

(Keep `targetId` as null for now — detail panel always shows the media item directly.)

- [ ] **Step 3: Replace variant loading useEffect**

```typescript
// Remove the variant list loading useEffect (loadVariants)
// Replace with lineage loading:
useEffect(() => {
  if (!media) return;
  mediaLineageList(media.id).then(setLineage).catch(() => setLineage({ parents: [], children: [] }));
}, [media?.id]);
```

- [ ] **Step 4: Remove TargetMenu component entirely**

Delete the `TargetMenu` component (lines ~124-181). Remove the target switching button (lines ~806-858).

The detail panel now always shows the current media item. No "原图/变体" switching.

- [ ] **Step 5: Add lineage chain display**

Below the thumbnail, add a compact lineage view:

```tsx
{/* Lineage chain */}
{(lineage.parents.length > 0 || lineage.children.length > 0) && (
  <div className="px-4 py-3 border-b border-[var(--color-border)]">
    <h4 className="text-[11px] font-medium text-[var(--color-text-muted)] mb-2">衍生关系</h4>

    {/* Parents */}
    {lineage.parents.length > 0 && (
      <div className="flex items-center gap-1.5 mb-1.5 flex-wrap">
        <span className="text-[10px] text-[var(--color-text-muted)] shrink-0">
          来源 ({lineage.parents.length})
        </span>
        {lineage.parents.map((p) => (
          <button
            key={p.media_id}
            onClick={() => onNavigate?.(p.media_id)}
            className="max-w-[120px] truncate rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-0.5 text-[10px] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] transition-colors"
            title={p.media_id}
          >
            {p.media_id.slice(0, 8)}...
          </button>
        ))}
      </div>
    )}

    {/* Children */}
    {lineage.children.length > 0 && (
      <div className="flex items-center gap-1.5 flex-wrap">
        <span className="text-[10px] text-[var(--color-text-muted)] shrink-0">
          衍生 ({lineage.children.length})
        </span>
        {lineage.children.map((c) => (
          <button
            key={c.media_id}
            onClick={() => onNavigate?.(c.media_id)}
            className="max-w-[120px] truncate rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-0.5 text-[10px] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] hover:text-[var(--color-text-primary)] transition-colors"
            title={c.media_id}
          >
            {c.media_id.slice(0, 8)}...
          </button>
        ))}
      </div>
    )}
  </div>
)}
```

- [ ] **Step 6: Remove variant form (generate/import modes) section**

Delete the variant creation form (lines ~1330-1562): preset templates, format/quality/resize controls, import drag-and-drop, and the generate button.

- [ ] **Step 7: Remove display_variant button logic**

Delete the display variant toggle buttons (lines ~1614-1644).

- [ ] **Step 8: Update ThumbnailPreview — remove variant thumbnail logic**

```tsx
// Before (with variant switching):
// if targetId is set, show variant thumbnail; otherwise show media thumb

// After (no targetId):
// Always show media thumb_256
function ThumbnailPreview({ media }: { media: Media }) {
  const thumbUrl = useThumbnail(media.id);
  // ... same LQIP blur transition
}
```

- [ ] **Step 9: Update ImagineDialog invocation**

```tsx
{/* AI Edit button */}
{showAiEdit && media && (
  <ImagineDialog
    mediaId={media.id}
    sourceMediaIds={[media.id]}
    sourceMediaPath={media.thumb_256 ?? ""}
    onClose={() => setShowAiEdit(false)}
  />
)}
```

- [ ] **Step 10: Update delete logic**

Remove the variant delete path (delete button when viewing a variant). Now there's only one delete code path: soft delete of the current media.

```tsx
// Delete button — always soft-delete current media
<button onClick={() => {
  if (!media) return;
  ConfirmDialog.show({
    title: "删除媒体",
    message: "确定要删除这张图片吗？",
    danger: true,
    onConfirm: async () => {
      await mediaSoftDelete(media.id);
      emit("collections-changed");
      onDeleted?.();
    },
  });
}}>
```

- [ ] **Step 11: Update collection add button — enable for all media**

Remove the `if (!targetId)` guard around "添加到集合" — now always enabled.

- [ ] **Step 12: Frontend type-check**

Run: `npx tsc --noEmit`
Expected: fix all type errors in DetailPanel before proceeding.

- [ ] **Step 13: Commit**

```bash
git add src/components/DetailPanel/DetailPanel.tsx
git commit -m "refactor: DetailPanel — remove variant UI, add lineage chain display"
```

---

### Task 10: Update ImagineDialog — sourceMediaIds prop

**Files:**
- Modify: `src/components/ImagineDialog/ImagineDialog.tsx`

- [ ] **Step 1: Update Props interface**

```typescript
interface Props {
  mediaId: string;
  sourceMediaIds: string[];   // replaces variantId
  sourceMediaPath?: string;   // replaces variantPath
  onClose: () => void;
}
```

- [ ] **Step 2: Update thumbnail display**

```typescript
const thumbUrl = sourceMediaPath ? convertFileSrc(sourceMediaPath) : useThumbnail(mediaId);
```

- [ ] **Step 3: Update handleSubmit — use sourceMediaIds**

```typescript
const handleSubmit = async () => {
  if (isComfy) {
    if (!selectedWorkflowId) return;
  } else {
    if (!prompt.trim()) return;
  }
  setSubmitting(true);
  setError(null);
  try {
    await imageQueueSubmitEdit(
      sourceMediaIds,
      isComfy ? (workflowValues.prompt || prompt.trim()) : prompt.trim(),
      aspectRatio,
      resolution,
      n,
      isComfy ? selectedWorkflowId : null,
    );
    record(prompt, aspectRatio, resolution);
    showToast("已加入队列");
    onClose();
  } catch (e) {
    setError(String(e));
  } finally {
    setSubmitting(false);
  }
};
```

- [ ] **Step 4: Add multi-image indicator for ComfyUI mode**

```tsx
{isComfy && sourceMediaIds.length > 1 && (
  <div className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-secondary)]">
    已选择 {sourceMediaIds.length} 张输入图
  </div>
)}
```

- [ ] **Step 5: Commit**

```bash
git add src/components/ImagineDialog/ImagineDialog.tsx
git commit -m "refactor: ImagineDialog — sourceMediaIds prop, multi-image indicator"
```

---

### Task 11: Update Gallery and TableView — BrowseItem changes

**Files:**
- Modify: `src/components/Gallery/Gallery.tsx`
- Modify: `src/components/TableView/TableView.tsx`

- [ ] **Step 1: Update Gallery — remove item_kind references**

```typescript
// Remove item_kind from the card rendering:
// Before: conditionally show "original" or "variant" badge
// After: show derivative badge if has_derivatives is true

{item.has_derivatives && (
  <span className="absolute bottom-1.5 right-1.5 z-10 rounded-full bg-[var(--color-accent)]/80 px-1.5 py-0.5 text-[10px] text-white backdrop-blur-sm">
    衍生
  </span>
)}
```

Remove any `item_kind` conditional rendering. Each card always links to `item.media_id` (which is now the same as `item.id`).

- [ ] **Step 2: Update TableView — remove item_kind column**

In the table column that showed "原图 / 变体" labels, remove or replace:

```tsx
// Remove item_kind column. Optionally add a "来源" column:
{
  header: "衍生",
  cell: (item: BrowseItem) => (
    item.parent_count > 0
      ? <span className="text-[11px] text-[var(--color-text-muted)]">{item.parent_count} 个来源</span>
      : <span className="text-[11px] text-[var(--color-text-muted)]">原始</span>
  ),
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/Gallery/Gallery.tsx src/components/TableView/TableView.tsx
git commit -m "refactor: Gallery/TableView — update BrowseItem display for lineage model"
```

---

### Task 12: Update frontend stores and hooks

**Files:**
- Modify: `src/stores/` — any store referencing variants
- Modify: `src/hooks/` — any hooks referencing variants

- [ ] **Step 1: Search for variant references**

Run: `grep -r "variant" src/stores/ src/hooks/ --include="*.ts" --include="*.tsx" -l`

- [ ] **Step 2: Update each file**

For each file found, replace variant imports and usages. Common patterns:

```typescript
// Before:
import type { Variant } from "@/types/variant";
// After: remove or replace with Media

// Before:
variantList(mediaId).then(setVariants);
// After: remove (lineage loaded in DetailPanel)

// Before:
setDisplayVariant(mediaId, variantId);
// After: remove entirely
```

- [ ] **Step 3: Commit**

```bash
git add src/stores/ src/hooks/
git commit -m "refactor: update stores and hooks for lineage model"
```

---

### Task 13: Update AiGenPage — BrowseItem changes

**Files:**
- Modify: `src/components/AiGenPage/AiGenPage.tsx`

- [ ] **Step 1: Update any BrowseItem usage**

The AiGenPage uses `imageQueueSubmitGenerate` which doesn't change. But check for any BrowseItem or variant references:

```bash
grep -n "variant\|BrowseItem\|item_kind\|display_variant" src/components/AiGenPage/AiGenPage.tsx
```

Expected: no matches (AiGenPage doesn't use BrowseItems or variants directly).

- [ ] **Step 2: Commit if changes needed, or skip**

---

### Task 14: Update Settings page — remove variant preset settings

**Files:**
- Modify: `src/components/Settings/Settings.tsx`

- [ ] **Step 1: Remove variant preset creation UI**

If the settings page has a section for managing Variant Presets, remove it.

- [ ] **Step 2: Commit**

```bash
git add src/components/Settings/Settings.tsx
git commit -m "refactor: remove variant preset management from Settings"
```

---

### Task 15: Write Rust unit tests for media_lineage

**Files:**
- Modify: `src-tauri/src/db/mod.rs` — add test module

- [ ] **Step 1: Write lineage tests**

```rust
#[cfg(test)]
mod lineage_tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&db).unwrap();
        db
    }

    fn insert_test_media(db: &Connection, id: &str, path: &str) {
        db.execute(
            "INSERT INTO media (id, file_path, file_name, file_size, format, media_type)
             VALUES (?1, ?2, 'test', 0, 'jpg', 'image')",
            params![id, path],
        ).unwrap();
    }

    #[test]
    fn test_lineage_insert_and_list() {
        let db = setup_test_db();
        insert_test_media(&db, "root", "/tmp/root.jpg");
        insert_test_media(&db, "child1", "/tmp/child1.jpg");
        insert_test_media(&db, "child2", "/tmp/child2.jpg");

        lineage_insert_path(&db, "root", "child1", "edit", None).unwrap();
        lineage_insert_path(&db, "root", "child2", "edit", None).unwrap();

        let graph = lineage_list_path(&db, "root").unwrap();
        assert_eq!(graph.parents.len(), 0);
        assert_eq!(graph.children.len(), 2);

        let graph = lineage_list_path(&db, "child1").unwrap();
        assert_eq!(graph.parents.len(), 1);
        assert_eq!(graph.parents[0].media_id, "root");
        assert_eq!(graph.children.len(), 0);
    }

    #[test]
    fn test_lineage_cycle_detection() {
        let db = setup_test_db();
        insert_test_media(&db, "a", "/tmp/a.jpg");
        insert_test_media(&db, "b", "/tmp/b.jpg");
        insert_test_media(&db, "c", "/tmp/c.jpg");

        // a → b → c  (valid DAG)
        lineage_insert_path(&db, "a", "b", "edit", None).unwrap();
        lineage_insert_path(&db, "b", "c", "edit", None).unwrap();

        // c → a would form a cycle
        assert!(lineage_would_cycle_path(&db, "c", "a").unwrap());
        // a → c already has a path (a→b→c), but a→c is not a cycle
        assert!(!lineage_would_cycle_path(&db, "a", "c").unwrap());
    }

    #[test]
    fn test_lineage_multi_parent() {
        let db = setup_test_db();
        insert_test_media(&db, "parent_a", "/tmp/a.jpg");
        insert_test_media(&db, "parent_b", "/tmp/b.jpg");
        insert_test_media(&db, "child", "/tmp/child.jpg");

        lineage_insert_path(&db, "parent_a", "child", "edit", None).unwrap();
        lineage_insert_path(&db, "parent_b", "child", "edit", None).unwrap();

        let graph = lineage_list_path(&db, "child").unwrap();
        assert_eq!(graph.parents.len(), 2);
    }

    #[test]
    fn test_media_is_root() {
        let db = setup_test_db();
        insert_test_media(&db, "root", "/tmp/root.jpg");
        insert_test_media(&db, "derived", "/tmp/derived.jpg");

        assert!(media_is_root(&db, "root").unwrap());
        lineage_insert_path(&db, "root", "derived", "edit", None).unwrap();
        assert!(!media_is_root(&db, "derived").unwrap());
    }

    #[test]
    fn test_media_lineage_cascade_on_delete() {
        let db = setup_test_db();
        insert_test_media(&db, "root", "/tmp/root.jpg");
        insert_test_media(&db, "child", "/tmp/child.jpg");

        lineage_insert_path(&db, "root", "child", "edit", None).unwrap();

        // Delete root
        db.execute("DELETE FROM media WHERE id = ?1", params!["root"]).unwrap();

        // Lineage row should be cascade-deleted
        let graph = lineage_list_path(&db, "child").unwrap();
        assert_eq!(graph.parents.len(), 0);
        // Child still exists (no cascade on child)
        let exists: bool = db.query_row(
            "SELECT COUNT(*) > 0 FROM media WHERE id = ?1",
            params!["child"],
            |r| r.get(0),
        ).unwrap();
        assert!(exists);
    }

    #[test]
    fn test_lineage_remove() {
        let db = setup_test_db();
        insert_test_media(&db, "root", "/tmp/root.jpg");
        insert_test_media(&db, "child", "/tmp/child.jpg");

        lineage_insert_path(&db, "root", "child", "edit", None).unwrap();
        lineage_remove_path(&db, "root", "child").unwrap();

        let graph = lineage_list_path(&db, "child").unwrap();
        assert_eq!(graph.parents.len(), 0);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test lineage_tests`
Expected: all 6 tests pass.

- [ ] **Step 3: Run existing tests to check for regressions**

Run: `cargo test --lib`
Expected: update any tests that reference variant types. Note failures and fix in Task 16.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/mod.rs
git commit -m "test: add media_lineage unit tests (6 tests)"
```

---

### Task 16: Fix existing tests broken by the refactor

**Files:**
- Modify: `src-tauri/src/db/mod.rs` — update search_tests, media_tests, etc.
- Modify: `src-tauri/src/commands/` — update command tests
- Modify: `tests/*.sh` — update CLI regression tests

- [ ] **Step 1: Find all test failures**

Run: `cargo test --lib 2>&1 | grep "FAILED"`

Identify:
- Tests that reference `variants` table or `Variant` struct
- Tests that reference `BrowseItem.item_kind` or `variant_id`
- Tests that reference `display_variant_id`

- [ ] **Step 2: Fix Rust tests**

For each failing test:
- Replace `variant_insert` calls with equivalent `media_insert_path` + `lineage_insert_path`
- Replace `item_kind == "original"` assertions with `parent_count == 0`
- Remove `display_variant_id` references

Example fix for search test:

```rust
// Before (variant-aware search):
let variant_id = Ulid::new().to_string();
db::variant_insert_path(&db, &Variant {
    id: variant_id.clone(),
    media_id: media_id.clone(),
    ...
}).unwrap();

// After:
let derived_id = Ulid::new().to_string();
db::media_insert_path(&db, &derived_id, "/tmp/derived.jpg", "", 0, "jpg", Some(800), Some(600), Some("image"), Some("ai-edited")).unwrap();
db::lineage_insert_path(&db, &media_id, &derived_id, "edit", None).unwrap();
```

- [ ] **Step 3: Update CLI regression tests**

Update `tests/variants-browse.sh` → rename to `tests/lineage-browse.sh`:

```bash
# Update test assertions:
# - Replace variant_list with media_lineage_list
# - Replace "original" kind checks with root-node checks
# - Replace display_variant tests with lineage chain tests
```

Run old tests to see what breaks:
```bash
cd src-tauri && bash ../tests/variants-browse.sh
```

Identify failing assertions and update.

- [ ] **Step 4: Run full test suite**

Run: `cargo test --lib && bash tests/*.sh`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/mod.rs tests/
git commit -m "test: fix existing tests for lineage model refactor"
```

---

### Task 17: Frontend Vitest tests — update and add

**Files:**
- Modify: `src/components/DetailPanel/DetailPanel.test.tsx` (if exists)
- Create: `src/components/DetailPanel/LineageChain.test.tsx`
- Modify: `src/components/ImagineDialog/ImagineDialog.test.tsx` (if exists)

- [ ] **Step 1: Run existing frontend tests**

Run: `npm test`
Expected: failures due to removed `variant.ts` imports and changed component APIs.

- [ ] **Step 2: Update component test mocks**

In each failing test file:
- Remove `variantList`, `variantDelete`, `mediaSetDisplayVariant` mocks
- Add `mediaLineageList` mock returning `{ parents: [], children: [] }`
- Update ImagineDialog test props from `variantId` to `sourceMediaIds`

```typescript
// Example: DetailPanel test mock update
vi.mock("@/lib/tauri", () => ({
  mediaLineageList: vi.fn().mockResolvedValue({ parents: [], children: [] }),
  mediaSoftDelete: vi.fn().mockResolvedValue(undefined),
  // ... other mocks
}));
```

- [ ] **Step 3: Add LineageChain test**

```tsx
// src/components/DetailPanel/LineageChain.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";

describe("LineageChain", () => {
  it("renders parents and children", () => {
    // Component receives LineageGraph and renders parent/child pills
  });

  it("shows nothing when no lineage", () => {
    // Empty graph = no rendering
  });

  it("clicking parent calls onNavigate", () => {
    // Click parent pill → onNavigate called with parent media_id
  });
});
```

- [ ] **Step 4: Run frontend tests**

Run: `npm test`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/components/DetailPanel/LineageChain.test.tsx
git commit -m "test: update frontend tests for lineage model"
```

---

### Task 18: Final integration test and cleanup

- [ ] **Step 1: Full build**

Run: `cargo build`
Expected: compiles with no warnings.

- [ ] **Step 2: Full test suite**

Run:
```bash
cargo test --lib
cargo clippy -- -D warnings
npm test
bash tests/*.sh
```

Expected: all pass.

- [ ] **Step 3: Remove dead code scan**

Run: `grep -r "display_variant" src-tauri/ src/ --include="*.rs" --include="*.ts" --include="*.tsx"`
Expected: no matches.

Run: `grep -r "variant_id" src-tauri/ src/ --include="*.rs" --include="*.ts" --include="*.tsx"`
Expected: no matches (except possibly in migration code or SQL strings for migration).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: final cleanup — remove dead code, verify tests pass"
```
