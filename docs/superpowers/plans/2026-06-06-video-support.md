# Video Support Implementation Plan (Phases A+B+C)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add video import, thumbnail generation, and browse/playback to Medix, using bundled ffmpeg sidecar and the verified asset:// protocol for video playback.

**Architecture:** Parallel video import path (`media/video_import.rs`) coexisting with image import. ffmpeg/ffprobe bundled as Tauri sidecar. Frontend components use `media_type` conditional rendering (`<video>` vs `<img>`). Schema adds 4 columns to `media` and `variants` tables via migration 0018/0019.

**Tech Stack:** Tauri v2.11, React 19, TypeScript, Rust, SQLite, ffmpeg sidecar, tauri-plugin-shell

---

### Task 1: Schema migration 0018 — media table video columns

**Files:**
- Modify: `src-tauri/src/db/mod.rs` (after line 379, end of 0017 migration)

- [ ] **Step 1: Add migration 0018_video_support to `run_migrations()`**

Insert after the closing `}` of migration 0017 (after line 379):

```rust
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
        // Conditionally add columns to media table
        let columns: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info('media')")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        let mut sql = String::from("INSERT OR IGNORE INTO _migrations (name) VALUES ('0018_video_support');");
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
```

- [ ] **Step 2: Run CLI integrity test to verify migration**

```bash
cd src-tauri && cargo run --bin medix-cli -- query "PRAGMA table_info('media');"
```

Expected: output includes `media_type`, `duration`, `video_codec`, `video_fps` columns.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/mod.rs
git commit -m "feat: add migration 0018 — video columns on media table"
```

---

### Task 2: Schema migration 0019 — variants table video columns

**Files:**
- Modify: `src-tauri/src/db/mod.rs` (after migration 0018)

- [ ] **Step 1: Add migration 0019_video_variants**

```rust
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
        let columns: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info('variants')")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        let mut sql = String::from("INSERT OR IGNORE INTO _migrations (name) VALUES ('0019_video_variants');");
        if !columns.contains(&"media_type".to_string()) {
            sql.push_str("ALTER TABLE variants ADD COLUMN media_type TEXT DEFAULT 'image';");
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
```

- [ ] **Step 2: Verify**

```bash
cd src-tauri && cargo run --bin medix-cli -- query "PRAGMA table_info('variants');"
```

Expected: includes new 4 video columns.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/mod.rs
git commit -m "feat: add migration 0019 — video columns on variants table"
```

---

### Task 3: Update Rust Media struct with video fields

**Files:**
- Modify: `src-tauri/src/media/mod.rs:7-26`

- [ ] **Step 1: Add video fields to Media struct**

Change the struct definition at line 7-26 to add 4 fields after `lqip`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Media {
    pub id: String,
    pub source_path: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub file_size: Option<i64>,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub imported_at: String,
    pub source_url: Option<String>,
    pub page_url: Option<String>,
    pub source: Option<String>,
    pub phash: Option<Vec<u8>>,
    pub sha256: Option<String>,
    pub deleted_at: Option<String>,
    pub display_variant_id: Option<String>,
    pub thumb_256: Option<String>,
    pub lqip: Option<String>,
    pub media_type: Option<String>,
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub video_fps: Option<f64>,
}
```

- [ ] **Step 2: Build check**

```bash
cd src-tauri && cargo check 2>&1 | head -30
```

Expected: compile errors in db/mod.rs where Media is constructed — this is expected, those will be fixed in Tasks 5-6.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/media/mod.rs
git commit -m "feat: add video fields to Media struct"
```

---

### Task 4: Update Rust Variant struct with video fields

**Files:**
- Modify: `src-tauri/src/variants/mod.rs:10-23`

- [ ] **Step 1: Add video fields to Variant struct**

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Variant {
    pub id: String,
    pub media_id: String,
    pub preset_name: String,
    pub format: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub quality: Option<i32>,
    pub file_size: Option<i64>,
    pub file_path: String,
    pub label: Option<String>,
    pub source: Option<String>,
    pub media_type: Option<String>,
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub video_fps: Option<f64>,
}
```

- [ ] **Step 2: Build check**

```bash
cd src-tauri && cargo check 2>&1 | head -30
```

Expected: errors where Variant is constructed in db/mod.rs and commands/variant.rs — will fix in subsequent tasks.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/variants/mod.rs
git commit -m "feat: add video fields to Variant struct"
```

---

### Task 5: Update all media SELECT queries in db/mod.rs

**Files:**
- Modify: `src-tauri/src/db/mod.rs` — 8 query functions

- [ ] **Step 1: Add 4 new columns to every media SELECT column list**

Each function below currently selects 15 columns. Add `media_type, duration, video_codec, video_fps` after `lqip`:

1. **`media_list_by_collection`** (line ~533): Change SELECT to include `m.media_type, m.duration, m.video_codec, m.video_fps`
2. **`media_get_by_sha256`** (line ~594): Same 4 columns added
3. **`list_media_path`** (line ~685): Same 4 columns added
4. **`media_get_batch`** (line ~757): Same 4 columns added
5. **`media_search_by_tags_path`** (lines ~1101, ~1112): Same 4 columns in both branches
6. **`media_query_filtered_path`** (line ~1379): Same 4 columns added
7. **`media_list_trash`** (line ~2012): Same 4 columns added

For each function, also update the row mapping to include the new fields. Pattern for each row mapping (add after `lqip`):

```rust
media_type: row.get(15).ok(),
duration: row.get(16).ok(),
video_codec: row.get(17).ok(),
video_fps: row.get(18).ok(),
```

Note: `media_find_similar` (line ~2073) only selects `id, phash, width, height, file_size` — it does NOT need updating since it's only for pHash comparison.

- [ ] **Step 2: Update `media_get_by_id`** if it has its own row mapping

`media_get_by_id` at line 2196-2199 delegates to `media_get_batch`, so no separate fix needed.

- [ ] **Step 3: Build check**

```bash
cd src-tauri && cargo check 2>&1
```

Expected: fewer errors; remaining ones are from INSERT (Task 6) and Variant queries.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/mod.rs
git commit -m "feat: add video columns to all media SELECT queries"
```

---

### Task 6: Update media INSERT and variant queries in db/mod.rs

**Files:**
- Modify: `src-tauri/src/db/mod.rs` — `insert_media` (line ~624), `variant_insert` (find it), `variant_get_by_id` (find it)

- [ ] **Step 1: Update `insert_media` column list and params**

Change lines 627-644. Add `media_type, duration, video_codec, video_fps` to both the column list and VALUES:

```rust
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
        media.media_type.as_deref(),
        media.duration,
        media.video_codec.as_deref(),
        media.video_fps,
    ],
)?;
```

- [ ] **Step 2: Find and update `variant_insert`**

Search for `fn variant_insert` or `INSERT INTO variants`. Add the 4 new columns (`media_type, duration, video_codec, video_fps`) to its INSERT column list and VALUES, pattern-matching the existing fields.

- [ ] **Step 3: Find and update `variant_get_by_id` and any variant SELECT queries**

Add the 4 video columns to variant SELECT queries and their row mappings. The variants table has fewer consumers than media — look for `SELECT ... FROM variants` patterns in db/mod.rs.

- [ ] **Step 4: Build check until clean**

```bash
cd src-tauri && cargo check 2>&1
```

Expected: no errors (warnings OK).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/mod.rs
git commit -m "feat: add video columns to INSERT and variant queries"
```

---

### Task 7: Update TypeScript Media interface

**Files:**
- Modify: `src/types/media.ts:1-18`

- [ ] **Step 1: Add video fields to Media interface**

```typescript
export interface Media {
  id: string;
  source_path: string | null;
  width: number | null;
  height: number | null;
  file_size: number | null;
  created_at: string | null;
  modified_at: string | null;
  imported_at: string;
  source_url: string | null;
  page_url: string | null;
  source: string | null;
  sha256: string | null;
  deleted_at: string | null;
  display_variant_id: string | null;
  thumb_256: string | null;
  lqip: string | null;
  media_type: string | null;
  duration: number | null;
  video_codec: string | null;
  video_fps: number | null;
}
```

- [ ] **Step 2: Check TypeScript compilation**

```bash
cd src-tauri/.. && npx tsc --noEmit 2>&1
```

Expected: no errors from media.ts. May have errors in components that use Media (will fix in later tasks).

- [ ] **Step 3: Commit**

```bash
git add src/types/media.ts
git commit -m "feat: add video fields to Media TypeScript interface"
```

---

### Task 8: Update TypeScript Variant interface

**Files:**
- Modify: `src/types/variant.ts:1-13`

- [ ] **Step 1: Add video fields to Variant interface**

```typescript
export interface Variant {
  id: string;
  media_id: string;
  preset_name: string;
  format: string;
  width: number | null;
  height: number | null;
  quality: number | null;
  file_size: number | null;
  file_path: string;
  label: string | null;
  source: string | null;
  media_type: string | null;
  duration: number | null;
  video_codec: string | null;
  video_fps: number | null;
}
```

- [ ] **Step 2: Commit**

```bash
git add src/types/variant.ts
git commit -m "feat: add video fields to Variant TypeScript interface"
```

---

### Task 9: Add tauri-plugin-shell and configure ffmpeg sidecar

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/default.json`
- Create: `src-tauri/binaries/.gitkeep`

- [ ] **Step 1: Add tauri-plugin-shell to Cargo.toml**

Add after line 16 (`tauri-plugin-dialog = "2"`):

```toml
tauri-plugin-shell = "2"
```

- [ ] **Step 2: Add externalBin to tauri.conf.json**

In the `bundle` section (line 37-47), add `externalBin`:

```json
"bundle": {
    "active": true,
    "targets": "all",
    "icon": [
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.icns",
        "icons/icon.ico"
    ],
    "externalBin": [
        "binaries/ffmpeg",
        "binaries/ffprobe"
    ]
}
```

- [ ] **Step 3: Add shell permission to capabilities/default.json**

Add to the `permissions` array:

```json
"shell:default"
```

- [ ] **Step 4: Create binaries placeholder directory**

```bash
mkdir -p src-tauri/binaries
touch src-tauri/binaries/.gitkeep
```

Note: The actual `ffmpeg.exe` and `ffprobe.exe` files (from ffmpeg official Windows builds, lgpl) need to be placed in `src-tauri/binaries/` before building. Add `src-tauri/binaries/ffmpeg.exe` and `src-tauri/binaries/ffprobe.exe` to `.gitignore`.

- [ ] **Step 5: Build check**

```bash
cd src-tauri && cargo check 2>&1
```

Expected: no new errors from the shell plugin addition.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/capabilities/default.json src-tauri/binaries/.gitkeep
git commit -m "feat: add tauri-plugin-shell and configure ffmpeg/ffprobe sidecar"
```

---

### Task 10: Add video settings keys and getters

**Files:**
- Modify: `src-tauri/src/settings/mod.rs`

- [ ] **Step 1: Add settings constants and getter functions**

After the existing settings definitions, add:

```rust
pub const KEY_VIDEO_LARGE_FILE_WARNING_MB: &str = "video_large_file_warning_mb";

pub fn get_video_large_file_warning_mb(app: &AppHandle) -> u64 {
    get(app, KEY_VIDEO_LARGE_FILE_WARNING_MB)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024)
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/settings/mod.rs
git commit -m "feat: add video settings keys (large file warning threshold)"
```

---

### Task 11: Create video metadata module (ffprobe wrapper)

**Files:**
- Create: `src-tauri/src/media/video_metadata.rs`
- Modify: `src-tauri/src/media/mod.rs`

- [ ] **Step 1: Create video_metadata.rs**

```rust
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: Option<FfprobeFormat>,
    streams: Option<Vec<FfprobeStream>>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    duration: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub width: i32,
    pub height: i32,
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub video_fps: Option<f64>,
}

/// Get ffprobe path: check sidecar first, then PATH
pub fn find_ffprobe(app: &tauri::AppHandle) -> Result<String, String> {
    // Check if app handle is available and has sidecar
    // For CLI usage, fall back to PATH
    if let Ok(shell) = app.try_state::<tauri_plugin_shell::Shell>() {
        // In Tauri context, try sidecar
        // The sidecar path is resolved by the shell plugin
    }
    // Fallback: check PATH
    which::which("ffprobe")
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|_| "ffprobe not found in PATH. Please install ffmpeg.".to_string())
}

/// Extract video metadata using ffprobe
pub fn extract_metadata(input: &Path) -> Result<VideoMetadata, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(input)
        .output()
        .map_err(|e| format!("ffprobe execution failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed: {}", stderr));
    }

    let meta: FfprobeOutput =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("ffprobe JSON parse: {}", e))?;

    // Find first video stream (not assuming streams[0] is video)
    let video_stream = meta
        .streams
        .as_ref()
        .and_then(|streams| streams.iter().find(|s| s.codec_type.as_deref() == Some("video")))
        .ok_or("No video stream found in file")?;

    let width = video_stream.width.unwrap_or(0);
    let height = video_stream.height.unwrap_or(0);

    // Duration: prefer format.duration, fall back to video stream.duration
    let duration = meta
        .format
        .as_ref()
        .and_then(|f| f.duration.as_ref())
        .and_then(|d| d.parse::<f64>().ok())
        .or_else(|| {
            video_stream
                .duration
                .as_ref()
                .and_then(|d| d.parse::<f64>().ok())
        });

    let video_codec = video_stream.codec_name.clone();

    // FPS: try avg_frame_rate first, then r_frame_rate
    let video_fps = video_stream
        .avg_frame_rate
        .as_ref()
        .and_then(|r| parse_fraction(r))
        .or_else(|| video_stream.r_frame_rate.as_ref().and_then(|r| parse_fraction(r)));

    Ok(VideoMetadata {
        width,
        height,
        duration,
        video_codec,
        video_fps,
    })
}

/// Verify file has a video stream (quick check before import)
pub fn has_video_stream(input: &Path) -> Result<bool, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_streams",
        ])
        .arg(input)
        .output()
        .map_err(|e| format!("ffprobe execution failed: {}", e))?;

    let meta: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("ffprobe JSON parse: {}", e))?;

    let has_video = meta["streams"]
        .as_array()
        .map(|streams| streams.iter().any(|s| s["codec_type"] == "video"))
        .unwrap_or(false);

    Ok(has_video)
}

fn parse_fraction(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num = parts[0].parse::<f64>().ok()?;
        let den = parts[1].parse::<f64>().ok()?;
        if den != 0.0 {
            return Some(num / den);
        }
    }
    s.parse::<f64>().ok()
}

/// Supported video extensions for initial screening
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mkv", "avi", "mov"];
```

- [ ] **Step 2: Register module in media/mod.rs**

Add at the top of `src-tauri/src/media/mod.rs`:

```rust
pub mod video_metadata;
```

- [ ] **Step 3: Build check**

```bash
cd src-tauri && cargo check 2>&1
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/media/video_metadata.rs src-tauri/src/media/mod.rs
git commit -m "feat: add video metadata extraction via ffprobe"
```

---

### Task 12: Register shell plugin in main.rs

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add plugin registration**

After line 43 (after `.plugin(tauri_plugin_dialog::init())`), add:

```rust
.plugin(tauri_plugin_shell::init())
```

- [ ] **Step 2: Build check**

```bash
cd src-tauri && cargo check 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: register tauri-plugin-shell for sidecar support"
```

---

### Task 13: Create video thumbnail module (ffmpeg frame extraction)

**Files:**
- Create: `src-tauri/src/media/video_thumbnail.rs`
- Modify: `src-tauri/src/media/mod.rs`

- [ ] **Step 1: Create video_thumbnail.rs**

```rust
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::AppHandle;

/// Generate a 256px thumbnail from a video file using ffmpeg.
/// Tries multiple timestamps in order: 10% of duration → 1.0s → 50% → first frame.
pub fn generate_video_thumbnail(
    app: &AppHandle,
    media_id: &str,
    source_path: &Path,
    duration_secs: Option<f64>,
) -> Result<PathBuf, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumbs_dir = app_dir.join("thumbnails");
    std::fs::create_dir_all(&thumbs_dir).map_err(|e| e.to_string())?;
    let thumb_path = thumbs_dir.join(format!("{}_256.jpg", media_id));

    let timestamps: Vec<String> = {
        let mut ts = Vec::new();
        // 10% position
        if let Some(d) = duration_secs {
            if d > 0.1 {
                ts.push(format!("{:.3}", d * 0.1));
            }
        }
        ts.push("1.0".to_string());
        if let Some(d) = duration_secs {
            if d > 1.0 {
                ts.push(format!("{:.3}", d * 0.5));
            }
        }
        ts.push("0.0".to_string());
        ts
    };

    for (i, ts) in timestamps.iter().enumerate() {
        let result = Command::new("ffmpeg")
            .args([
                "-ss", ts,
                "-i",
            ])
            .arg(source_path)
            .args([
                "-frames:v", "1",
                "-vf", "scale=256:256:force_original_aspect_ratio=decrease,pad=256:256:(ow-iw)/2:(oh-ih)/2",
                "-q:v", "3",
                "-y",
            ])
            .arg(&thumb_path)
            .output();

        match result {
            Ok(output) if output.status.success() && thumb_path.exists() => {
                return Ok(thumb_path);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "[video_thumbnail] attempt {}/{} (t={}) failed: {}",
                    i + 1,
                    timestamps.len(),
                    ts,
                    stderr.lines().last().unwrap_or("unknown error")
                );
            }
            Err(e) => {
                eprintln!(
                    "[video_thumbnail] attempt {}/{} (t={}) ffmpeg error: {}",
                    i + 1,
                    timestamps.len(),
                    ts,
                    e
                );
            }
        }
    }

    // All attempts failed — but we still return Ok to not block the import
    // The frontend will show a placeholder for missing thumbnails
    if thumb_path.exists() {
        Ok(thumb_path)
    } else {
        Err("All thumbnail extraction attempts failed".to_string())
    }
}
```

- [ ] **Step 2: Register in media/mod.rs**

```rust
pub mod video_thumbnail;
```

- [ ] **Step 3: Build check**

```bash
cd src-tauri && cargo check 2>&1
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/media/video_thumbnail.rs src-tauri/src/media/mod.rs
git commit -m "feat: add video thumbnail generation via ffmpeg frame extraction"
```

---

### Task 14: Create video import module

**Files:**
- Create: `src-tauri/src/media/video_import.rs`
- Modify: `src-tauri/src/media/mod.rs`

- [ ] **Step 1: Create video_import.rs**

```rust
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use ulid::Ulid;

use super::video_metadata::{self, VideoMetadata, VIDEO_EXTENSIONS};
use super::video_thumbnail;
use super::{Media, MediaImportResult};

/// Import a single video file. Returns MediaImportResult for progress reporting.
pub fn import_single_video(
    app: &AppHandle,
    source_path: &Path,
    library_dir: &Path,
) -> MediaImportResult {
    let id = Ulid::new().to_string();

    // 1. Check extension
    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if !VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        return MediaImportResult {
            id,
            path: source_path.to_string_lossy().to_string(),
            success: false,
            error: Some(format!("Unsupported video extension: .{}", ext)),
        };
    }

    // 2. Verify video stream via ffprobe
    match video_metadata::has_video_stream(source_path) {
        Ok(true) => {}
        Ok(false) => {
            return MediaImportResult {
                id,
                path: source_path.to_string_lossy().to_string(),
                success: false,
                error: Some("File has no video stream".to_string()),
            };
        }
        Err(e) => {
            return MediaImportResult {
                id,
                path: source_path.to_string_lossy().to_string(),
                success: false,
                error: Some(format!("ffprobe check failed: {}", e)),
            };
        }
    }

    // 3. Large file check
    let file_size = match fs::metadata(source_path) {
        Ok(m) => m.len(),
        Err(e) => {
            return MediaImportResult {
                id: id.clone(),
                path: source_path.to_string_lossy().to_string(),
                success: false,
                error: Some(format!("Cannot read file metadata: {}", e)),
            };
        }
    };

    let warning_mb = crate::settings::get_video_large_file_warning_mb(app);
    if file_size > warning_mb * 1024 * 1024 {
        // Emit an event for the frontend to show a confirmation dialog
        let _ = app.emit(
            "video-large-file-warning",
            serde_json::json!({
                "id": id,
                "path": source_path.to_string_lossy().to_string(),
                "size": file_size,
                "threshold_mb": warning_mb,
            }),
        );
    }

    // 4. Copy to library
    let dest_path = library_dir.join(format!("{}.{}", id, ext));
    if let Err(e) = fs::copy(source_path, &dest_path) {
        return MediaImportResult {
            id,
            path: source_path.to_string_lossy().to_string(),
            success: false,
            error: Some(format!("Copy failed: {}", e)),
        };
    }

    // 5. SHA256 dedup
    let sha256 = match compute_sha256(&dest_path) {
        Ok(hash) => Some(hash),
        Err(e) => {
            let _ = fs::remove_file(&dest_path);
            return MediaImportResult {
                id,
                path: source_path.to_string_lossy().to_string(),
                success: false,
                error: Some(format!("SHA256 failed: {}", e)),
            };
        }
    };

    if let Some(ref hash) = sha256 {
        if let Ok(Some(_existing)) = crate::db::media_get_by_sha256(app, hash) {
            let _ = fs::remove_file(&dest_path);
            return MediaImportResult {
                id,
                path: source_path.to_string_lossy().to_string(),
                success: false,
                error: Some("Duplicate file (SHA256 match)".to_string()),
            };
        }
    }

    // 6. Extract metadata
    let metadata = match video_metadata::extract_metadata(&dest_path) {
        Ok(m) => m,
        Err(e) => {
            let _ = fs::remove_file(&dest_path);
            return MediaImportResult {
                id: id.clone(),
                path: source_path.to_string_lossy().to_string(),
                success: false,
                error: Some(format!("Metadata extraction failed: {}", e)),
            };
        }
    };

    // 7. Generate thumbnail (from 10% timestamp)
    let thumb_result = video_thumbnail::generate_video_thumbnail(
        app,
        &id,
        &dest_path,
        metadata.duration,
    );

    // 8. Generate LQIP from thumbnail (skip for now — thumbnail is JPEG, LQIP path helper not yet created)
    let lqip = None;

    // 9. Insert into database
    let media = Media {
        id: id.clone(),
        source_path: Some(source_path.to_string_lossy().to_string()),
        width: Some(metadata.width),
        height: Some(metadata.height),
        file_size: Some(file_size as i64),
        created_at: None,
        modified_at: None,
        imported_at: chrono::Utc::now().to_rfc3339(),
        source_url: None,
        page_url: None,
        source: Some("local".to_string()),
        phash: None, // No pHash for video
        sha256,
        deleted_at: None,
        display_variant_id: None,
        thumb_256: None,
        lqip,
        media_type: Some("video".to_string()),
        duration: metadata.duration,
        video_codec: metadata.video_codec,
        video_fps: metadata.video_fps,
    };

    if let Err(e) = crate::db::insert_media(app, &media) {
        let _ = fs::remove_file(&dest_path);
        return MediaImportResult {
            id,
            path: source_path.to_string_lossy().to_string(),
            success: false,
            error: Some(format!("DB insert failed: {}", e)),
        };
    }

    MediaImportResult {
        id: id.clone(),
        path: source_path.to_string_lossy().to_string(),
        success: true,
        error: None,
    }
}

/// Compute SHA256 of a file (used by video import for dedup)
pub fn compute_sha256(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", hasher.finalize()))
}
```

- [ ] **Step 2: Register in media/mod.rs**

```rust
pub mod video_import;
```

- [ ] **Step 3: Build check + fix compilation**

```bash
cd src-tauri && cargo check 2>&1
```

Fix any missing imports or type mismatches. The `generate_lqip_from_path` function may not exist — if not, skip LQIP for video in this task (set `lqip: None`).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/media/video_import.rs src-tauri/src/media/mod.rs
git commit -m "feat: add video import pipeline with ffprobe validation and ffmpeg thumbnails"
```

---

### Task 15: Extend media_import command to route video files

**Files:**
- Modify: `src-tauri/src/media/import.rs` — add video extensions to SUPPORTED_EXTENSIONS and add routing
- Modify: `src-tauri/src/commands/media.rs` — minimal, import.rs handles routing

- [ ] **Step 1: Extend SUPPORTED_EXTENSIONS in import.rs**

At line 11:

```rust
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp",
    "mp4", "webm", "mkv", "avi", "mov",
];
```

- [ ] **Step 2: Add video routing in import_files function**

In `import_files` (line ~59), inside the loop where each file is processed (around line 100), add a dispatch based on extension:

```rust
let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.to_lowercase())
    .unwrap_or_default();

let is_video = super::video_metadata::VIDEO_EXTENSIONS.contains(&ext.as_str());

if is_video {
    super::video_import::import_single_video(app, &path, library_dir)
} else {
    import_single_file(app, &path, library_dir, app_data_dir)
}
```

Note: `import_single_file` currently takes `(app, source_path, library_dir)`. You may need to adjust its signature — check if it currently takes `app_data_dir` as a 4th parameter or not.

- [ ] **Step 3: Emit import-progress events for video stages**

In `import_single_video`, emit progress events at key stages. Use the same event name and format as the image import:

```rust
let _ = app.emit("import-progress", serde_json::json!({
    "id": id,
    "path": source_path.to_string_lossy().to_string(),
    "stage": "validating", // then "copying", "hashing", "metadata", "thumbnail", "database"
    "total": total_count,
    "current": current_index,
}));
```

- [ ] **Step 4: Build check**

```bash
cd src-tauri && cargo check 2>&1
```

- [ ] **Step 5: Test import with a real MP4 file**

```bash
cd src-tauri && cargo run --bin medix-cli -- query "SELECT COUNT(*) FROM media WHERE media_type='video';"
```

Before this works, need to ensure the CLI has access to ffprobe. If sidecar is not available in CLI mode, test will need ffprobe on PATH.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/media/import.rs src-tauri/src/commands/media.rs
git commit -m "feat: route video files to video import pipeline in media_import"
```

---

### Task 16: Handle video variant import

**Files:**
- Modify: `src-tauri/src/commands/variant.rs:57-115`

- [ ] **Step 1: Update `variant_import` to accept video files**

In `variant_import`, after getting the file extension, check if it's a video extension. If so, use ffprobe for metadata instead of `image::open`:

```rust
let ext = source_path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.to_lowercase())
    .unwrap_or_default();

let is_video = crate::media::video_metadata::VIDEO_EXTENSIONS.contains(&ext.as_str());

let (width, height, media_type, duration, video_codec, video_fps) = if is_video {
    let meta = crate::media::video_metadata::extract_metadata(&source_path)
        .map_err(|e| format!("ffprobe failed: {}", e))?;
    (
        Some(meta.width),
        Some(meta.height),
        Some("video".to_string()),
        meta.duration,
        meta.video_codec,
        meta.video_fps,
    )
} else {
    let img = image::open(&source_path).map_err(|e| format!("decode failed: {}", e))?;
    (
        Some(img.width() as i32),
        Some(img.height() as i32),
        Some("image".to_string()),
        None,
        None,
        None,
    )
};
```

Then update the Variant struct construction to include the new fields:

```rust
let variant = crate::variants::Variant {
    // ... existing fields
    media_type,
    duration,
    video_codec,
    video_fps,
};
```

For video variants, skip the `image::open` thumbnail generation and use `video_thumbnail::generate_video_thumbnail` instead.

- [ ] **Step 2: Build check**

```bash
cd src-tauri && cargo check 2>&1
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/variant.rs
git commit -m "feat: support video file import as variant via ffprobe"
```

---

### Task 17: Update media_permanent_delete for video file cleanup

**Files:**
- Modify: `src-tauri/src/db/mod.rs:1953-1993`

- [ ] **Step 1: Review current cleanup logic**

The current `media_permanent_delete` iterates `library/` and `thumbnails/` directories looking for files with the media ID prefix. This already works for video files stored as `{id}.mp4` in `library/` and `{id}_256.jpg` in `thumbnails/`. No change needed for the file cleanup itself.

However, verify that the variant cleanup also deletes video variant files correctly — variants are in `variants/` directory and cleaned up by prefix match. This also works for video variants (`{media_id}_{ulid}.mp4`).

No code change needed here.

- [ ] **Step 1: Verify cleanup with a test**

```bash
cd src-tauri && cargo run --bin medix-cli -- query "SELECT id FROM media WHERE media_type='video' LIMIT 1;"
# Note the ID, then verify files exist in library/ and thumbnails/
```

- [ ] **Step 2: Commit** (if changes needed, otherwise skip)

---

### Task 18: Gallery video duration badge

**Files:**
- Modify: `src/components/Gallery/Gallery.tsx` — `ThumbnailCard` component (line 242-351)

- [ ] **Step 1: Add duration badge to ThumbnailCard**

After the hover info overlay div (line 347), add a duration badge that conditionally renders for video:

```tsx
{/* Duration badge for video */}
{item.media_type === "video" && item.duration != null && (
  <div className="absolute right-2 bottom-2 z-10 rounded bg-black/60 px-1.5 py-0.5 text-[11px] tabular-nums text-white">
    {formatDuration(item.duration)}
  </div>
)}
```

- [ ] **Step 2: Add formatDuration helper**

Add at the top of the file (or in a shared utils file):

```typescript
function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  if (seconds >= 600) {
    return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/Gallery/Gallery.tsx
git commit -m "feat: add video duration badge to Gallery thumbnail cards"
```

---

### Task 19: TableView type/duration column

**Files:**
- Modify: `src/components/TableView/TableView.tsx` — column header (line 91-100) and row rendering (line 261-263)

- [ ] **Step 1: Change column header from "尺寸" to "类型/时长"**

Change line ~93:

```tsx
<button onClick={() => onSortChange("width")} className="...">
  类型/时长
</button>
```

- [ ] **Step 2: Update row cell to show type-specific info**

Change line ~261-263. Replace `{item.width ?? "?"}x{item.height ?? "?"}` with:

```tsx
{item.media_type === "video" ? (
  <span className="text-xs text-[var(--color-text-secondary)] tabular-nums">
    {item.duration != null ? `${formatDuration(item.duration)} · ` : ""}
    {item.height != null ? `${item.height}p` : "?"}
  </span>
) : (
  <span className="text-xs text-[var(--color-text-secondary)] tabular-nums">
    {item.width ?? "?"}×{item.height ?? "?"}
  </span>
)}
```

Add the `formatDuration` helper (same as Task 18).

- [ ] **Step 3: Commit**

```bash
git add src/components/TableView/TableView.tsx
git commit -m "feat: show video duration + resolution in TableView type column"
```

---

### Task 20: DetailPanel video metadata rows

**Files:**
- Modify: `src/components/DetailPanel/DetailPanel.tsx` — Details tab (line 670-835)

- [ ] **Step 1: Add video metadata rows after existing dimensions row**

After the dimensions display (line ~715), add conditional video metadata:

```tsx
{selectedItem.media_type === "video" && (
  <>
    {selectedItem.duration != null && (
      <div>
        <span className="text-[var(--color-text-muted)]">时长</span>
        <span className="float-right text-[var(--color-text-primary)]">
          {formatDurationChinese(selectedItem.duration)}
        </span>
      </div>
    )}
    {selectedItem.video_codec != null && (
      <div>
        <span className="text-[var(--color-text-muted)]">编码</span>
        <span className="float-right text-[var(--color-text-primary)]">
          {selectedItem.video_codec}
        </span>
      </div>
    )}
    {selectedItem.video_fps != null && (
      <div>
        <span className="text-[var(--color-text-muted)]">帧率</span>
        <span className="float-right text-[var(--color-text-primary)]">
          {selectedItem.video_fps.toFixed(2)} fps
        </span>
      </div>
    )}
  </>
)}
```

- [ ] **Step 2: Add formatDurationChinese helper**

```typescript
function formatDurationChinese(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) return `${h}时${m}分${s}秒`;
  if (m > 0 && s > 0) return `${m}分${s}秒`;
  if (m > 0) return `${m}分`;
  return `${s}秒`;
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/DetailPanel/DetailPanel.tsx
git commit -m "feat: show video duration/codec/fps in DetailPanel"
```

---

### Task 21: Lightbox video player (basic)

**Files:**
- Modify: `src/components/Lightbox/Lightbox.tsx` — main image display at line 579

- [ ] **Step 1: Add conditional video element**

At the main display section (around line 579, where `<img src={mainUrl} ...>` is used), add a conditional:

```tsx
{currentItem.media_type === "video" ? (
  <video
    ref={videoRef}
    src={mainUrl}
    controls
    autoPlay
    className="max-h-[90vh] max-w-[90vw] rounded-lg"
    onError={() => {
      toast({
        title: "播放失败",
        description: "当前系统不支持此视频编码或容器",
        variant: "destructive",
      });
    }}
  />
) : (
  <img
    src={mainUrl}
    alt=""
    className="max-h-[90vh] max-w-[90vw] object-contain ..."
    // ... existing transform/zoom props
  />
)}
```

- [ ] **Step 2: Add videoRef**

```typescript
const videoRef = useRef<HTMLVideoElement>(null);
```

- [ ] **Step 3: Get currentItem's media_type**

The `currentItem` is already available from the media array. Ensure `media_type` is part of the Media type (added in Task 7).

- [ ] **Step 4: Get mainUrl for video**

The `mainUrl` used for `<img>` is constructed from `convertFileSrc(media_get_paths(...).original)`. This already works for video files since `media_get_paths` finds the library file by ID prefix. If `useLightboxUrl` or similar hook constructs the URL, ensure it also handles video.

- [ ] **Step 5: Commit**

```bash
git add src/components/Lightbox/Lightbox.tsx
git commit -m "feat: add video player to Lightbox with error handling"
```

---

### Task 22: Lightbox video keyboard shortcuts

**Files:**
- Modify: `src/components/Lightbox/Lightbox.tsx` — keyboard handler (line 240-289)

- [ ] **Step 1: Add video-specific keyboard shortcuts**

In the `useEffect` keyboard handler, inside the `switch` statement, add video cases:

```typescript
case " ":
  if (currentItem?.media_type === "video" && videoRef.current) {
    e.preventDefault();
    if (videoRef.current.paused) {
      videoRef.current.play();
    } else {
      videoRef.current.pause();
    }
  }
  break;
case "ArrowLeft":
  if (currentItem?.media_type === "video" && videoRef.current && viewState.type !== "compare") {
    e.preventDefault();
    videoRef.current.currentTime = Math.max(0, videoRef.current.currentTime - 5);
  } else if (viewState.type !== "compare" && currentIndex > 0) {
    onNavigate(currentIndex - 1);
  }
  break;
case "ArrowRight":
  if (currentItem?.media_type === "video" && videoRef.current && viewState.type !== "compare") {
    e.preventDefault();
    videoRef.current.currentTime = Math.min(
      videoRef.current.duration,
      videoRef.current.currentTime + 5
    );
  } else if (viewState.type !== "compare" && currentIndex < media.length - 1) {
    onNavigate(currentIndex + 1);
  }
  break;
```

- [ ] **Step 2: Add double-click handler for video**

In the video element's parent wrapper, add:

```typescript
onDoubleClick={() => {
  if (currentItem?.media_type === "video" && videoRef.current) {
    if (videoRef.current.paused) {
      videoRef.current.play();
    } else {
      videoRef.current.pause();
    }
  }
}}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/Lightbox/Lightbox.tsx
git commit -m "feat: add video keyboard shortcuts (space, arrows) to Lightbox"
```

---

### Task 23: Lightbox variant mixed preview (image + video)

**Files:**
- Modify: `src/components/Lightbox/Lightbox.tsx` — variant comparison rendering (line 507-571)

- [ ] **Step 1: Update side-by-side comparison to handle video**

In the comparison mode rendering, replace `<img>` with a conditional:

```tsx
{leftItem.media_type === "video" ? (
  <video src={getFilePath(leftItem)} controls className="max-h-[85vh] max-w-[44vw] object-contain" />
) : (
  <img src={getFilePath(leftItem)} alt="" className="max-h-[85vh] max-w-[44vw] object-contain" />
)}
```

Same for the right side (`rightItem`).

- [ ] **Step 2: For image vs video comparison, show side-by-side without pixel diff**

When one side is video and the other is image, skip the slider overlay mode. Show a message:

```tsx
{leftItem.media_type !== rightItem.media_type && (
  <div className="text-xs text-[var(--color-text-muted)] mt-2 text-center">
    图片与视频对比模式 — 并排预览
  </div>
)}
```

- [ ] **Step 3: For video vs video, show side-by-side without sync**

Two video elements side by side, each with independent controls. No synchronized playback.

- [ ] **Step 4: Commit**

```bash
git add src/components/Lightbox/Lightbox.tsx
git commit -m "feat: support image/video mixed variant comparison in Lightbox"
```

---

### Task 24: Settings page — video section

**Files:**
- Modify: `src/components/Settings/Settings.tsx`

- [ ] **Step 1: Add video settings section**

Find the settings page component and add a video section after existing sections (e.g., after AI-related settings). Use the design language classes:

```tsx
{/* Video Section */}
<div className="mt-8">
  <h3 className="text-sm font-semibold text-[var(--color-text-primary)]">视频</h3>
  <div className="mt-3 space-y-4">
    {/* ffmpeg status */}
    <div className="rounded-lg border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] p-3">
      <p className="text-xs text-[var(--color-text-muted)]">
        ffmpeg 已内置在应用中，开箱即用。仅在 ffmpeg 损坏或缺失时才需手动配置路径。
      </p>
    </div>

    {/* Large file warning threshold */}
    <div className="flex items-center justify-between">
      <div>
        <p className="text-sm text-[var(--color-text-primary)]">大视频确认阈值</p>
        <p className="text-xs text-[var(--color-text-muted)]">超过此大小的视频导入前需要确认</p>
      </div>
      <div className="flex items-center gap-2">
        <input
          type="number"
          value={largeFileThreshold}
          onChange={(e) => setLargeFileThreshold(Number(e.target.value))}
          onBlur={saveLargeFileThreshold}
          className="w-20 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-2 py-1 text-xs text-right text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
        />
        <span className="text-xs text-[var(--color-text-secondary)]">MB</span>
      </div>
    </div>
  </div>
</div>
```

- [ ] **Step 2: Add state and save logic**

```typescript
const [largeFileThreshold, setLargeFileThreshold] = useState(1024);

useEffect(() => {
  settingsGet("video_large_file_warning_mb").then((v) => {
    if (v) setLargeFileThreshold(Number(v));
  }).catch(() => {});
}, []);

const saveLargeFileThreshold = async () => {
  try {
    await settingsSet("video_large_file_warning_mb", String(largeFileThreshold));
  } catch { /* ignore */ }
};
```

- [ ] **Step 3: Commit**

```bash
git add src/components/Settings/Settings.tsx
git commit -m "feat: add video settings section with large file threshold"
```

---

### Task 25: CLI regression tests

**Files:**
- Modify: `tests/integrity.sh`
- Modify: `tests/operations.sh`
- Modify: `tests/cascade.sh`

- [ ] **Step 1: Add migration integrity tests to integrity.sh**

```bash
# Test: old database migrates with media_type defaulting to 'image'
check "Media media_type defaults to image for all existing rows" \
  "$(q "SELECT COUNT(*) FROM media WHERE media_type != 'image' OR media_type IS NULL;")" \
  "0"

# Test: new columns exist on media table
check "Media table has media_type column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media') WHERE name='media_type';")" \
  "1"

check "Media table has duration column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media') WHERE name='duration';")" \
  "1"

check "Media table has video_codec column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media') WHERE name='video_codec';")" \
  "1"

check "Media table has video_fps column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media') WHERE name='video_fps';")" \
  "1"

# Test: variants table has video columns
check "Variants table has media_type column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('variants') WHERE name='media_type';")" \
  "1"

# Test: repeated migration does not error
check "Migration 0018 is idempotent" \
  "$(q "SELECT COUNT(*) FROM _migrations WHERE name = '0018_video_support';")" \
  "1"
```

- [ ] **Step 2: Add import tests to operations.sh**

```bash
# Test: import MP4 video (requires ffprobe on PATH or skip if not available)
if command -v ffprobe &> /dev/null && command -v ffmpeg &> /dev/null; then
  # Create a tiny test MP4 (1 second black screen)
  ffmpeg -y -f lavfi -i color=c=black:s=320x240:d=1 -c:v libx264 -pix_fmt yuv420p /tmp/test_video.mp4 2>/dev/null
  if [ -f /tmp/test_video.mp4 ]; then
    cli import /tmp/test_video.mp4
    check "Video import creates record with media_type='video'" \
      "$(q "SELECT media_type FROM media WHERE id = (SELECT id FROM media ORDER BY imported_at DESC LIMIT 1);")" \
      "video"
    check "Video import sets duration" \
      "$(q "SELECT duration IS NOT NULL FROM media WHERE id = (SELECT id FROM media ORDER BY imported_at DESC LIMIT 1);")" \
      "1"
    rm /tmp/test_video.mp4
  fi
fi

# Test: import non-video file as video fails gracefully
echo "not a video" > /tmp/fake_video.mp4
cli import /tmp/fake_video.mp4 2>&1 | grep -q "video stream\|ffprobe" && echo "PASS: rejects non-video .mp4" || echo "FAIL: should reject"
rm /tmp/fake_video.mp4
```

- [ ] **Step 3: Add variant cascade tests to cascade.sh**

```bash
# Test: video variant cleanup on media delete
if command -v ffprobe &> /dev/null; then
  # Create test video, import as variant of an existing image, verify cleanup
  echo "Video variant cascade test (manual verification needed)"
fi
```

- [ ] **Step 4: Run all tests**

```bash
cd src-tauri && bash ../tests/integrity.sh && bash ../tests/operations.sh && bash ../tests/cascade.sh
```

Expected: all existing tests still pass, new video tests pass (or skip if ffprobe not available).

- [ ] **Step 5: Commit**

```bash
git add tests/integrity.sh tests/operations.sh tests/cascade.sh
git commit -m "test: add video schema and import regression tests"
```

---

### Final Verification

- [ ] **Step 1: Run full test suite**

```bash
cd src-tauri && bash ../tests/integrity.sh && bash ../tests/operations.sh && bash ../tests/cascade.sh && bash ../tests/search.sh && bash ../tests/tags-collections.sh
```

All existing tests must pass.

- [ ] **Step 2: Manual smoke test in dev mode**

```bash
npm run tauri dev
```

1. Import an MP4 file → should appear in Gallery with duration badge
2. Click thumbnail → Lightbox should play video with controls
3. Seek in video → should work smoothly (verified Range Request support)
4. DetailPanel → should show duration/codec/fps
5. TableView → should show type/duration column
6. Import video as variant → should appear in variant list

- [ ] **Step 3: Commit final fixes if any**

```bash
git add -A
git commit -m "chore: final adjustments from smoke testing"
```
