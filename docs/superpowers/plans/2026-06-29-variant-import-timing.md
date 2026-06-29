# Variant Import Timing Instrumentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add phase-level timing logs to the `variant_import` Tauri command so bottlenecks are visible.

**Architecture:** Use `std::time::Instant` inside `src-tauri/src/commands/variant.rs` to measure each major phase of `variant_import` and emit `println!` summaries in the same format used by `media/import.rs`. No new modules, events, or dependencies.

**Tech Stack:** Rust, Tauri, `std::time::Instant`

---

### File Structure

- **Modify:** `src-tauri/src/commands/variant.rs`
  - Add `use std::time::Instant;` to the existing imports.
  - Instrument `variant_import` with per-phase timers and a total timer.

---

### Task 1: Add imports and total timer

**Files:**
- Modify: `src-tauri/src/commands/variant.rs:1-8`

- [ ] **Step 1: Add `Instant` import**

Add `use std::time::Instant;` to the top of the file, next to the existing `use std::...` lines.

```rust
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Instant;
use tauri::{command, AppHandle, Manager};
```

- [ ] **Step 2: Start total timer at the top of `variant_import`**

Insert this as the first line inside `variant_import`:

```rust
let t_total = Instant::now();
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/variant.rs
git commit -m "chore(variant): add Instant import and total timer"
```

---

### Task 2: Instrument each major phase

**Files:**
- Modify: `src-tauri/src/commands/variant.rs:68-179`

- [ ] **Step 1: Add a phase helper macro/closure to reduce repetition**

At the top of `variant_import`, after `let t_total = Instant::now();`, add a small closure:

```rust
let log_phase = |phase: &str, t: &Instant| {
    println!(
        "[variant_import] {} phase={} duration_ms={}",
        media_id,
        phase,
        t.elapsed().as_millis()
    );
};
```

This closure captures `media_id` and reuses the output format from `media/import.rs`.

- [ ] **Step 2: Time format detection phase**

Replace the current detection block (lines 68-95) with a timed version. Keep existing `eprintln!` debug lines.

```rust
// Detect image format from magic bytes (consistent with normal import),
// fall back to extension check for video files.
let t_detect = Instant::now();
let mut first_bytes = vec![0u8; 12];
let mut f = fs::File::open(src).map_err(|e| format!("Failed to open source: {}", e))?;
let n = f.read(&mut first_bytes).unwrap_or(0);
drop(f);
eprintln!("[variant_import] path={}, read={} bytes, magic={:02x?}",
    source_path, n, &first_bytes[..n.min(12)]);

let ext;
let is_image;
let is_video;
if let Some(detected) = crate::media::import::detect_format_from_bytes(&first_bytes[..n]) {
    ext = detected.to_string();
    is_image = true;
    is_video = false;
    eprintln!("[variant_import] detected image format: {}", ext);
} else {
    // Not a recognized image format — try video by extension
    ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    is_image = false;
    is_video = crate::media::video_metadata::VIDEO_EXTENSIONS.contains(&ext.as_str());
    eprintln!("[variant_import] not an image, ext={}, is_video={}", ext, is_video);
}
log_phase("detect", &t_detect);
```

- [ ] **Step 3: Time file copy phase**

Wrap the copy operation with a timer. Insert `let t_copy = Instant::now();` before `fs::copy`, and log after:

```rust
let t_copy = Instant::now();
fs::copy(src, &dest).map_err(|e| e.to_string())?;
log_phase("copy", &t_copy);
```

- [ ] **Step 4: Time metadata extraction phase**

Wrap the `if is_video { ... } else { ... }` metadata extraction block with a timer:

```rust
let t_metadata = Instant::now();
let (width, height, media_type, duration, video_codec, video_fps) = if is_video {
    // ... existing video branch ...
} else {
    // ... existing image branch ...
};
log_phase("metadata", &t_metadata);
```

Do not change the branch bodies.

- [ ] **Step 5: Time DB insert phase**

```rust
let t_db = Instant::now();
db::variant_insert(&app, &variant).map_err(|e| e.to_string())?;
log_phase("db_insert", &t_db);
```

- [ ] **Step 6: Time thumbnail generation phase**

```rust
let t_thumb = Instant::now();
if is_video {
    if let Err(e) = crate::media::video_thumbnail::generate_video_thumbnail(
        &app, &variant.id, Path::new(&variant.file_path), variant.duration,
    ) {
        eprintln!("[variant] video thumbnail failed for imported {}: {}", variant.id, e);
    }
} else if let Err(e) = media::thumbnail::generate_variant_thumbnail(
    &app, &variant.id, Path::new(&variant.file_path),
) {
    eprintln!("[variant] thumbnail failed for imported {}: {}", variant.id, e);
}
log_phase("thumbnail", &t_thumb);
```

- [ ] **Step 7: Log total time before returning**

Right before `Ok(variant)`:

```rust
log_phase("total", &t_total);
Ok(variant)
```

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/variant.rs
git commit -m "feat(variant): add per-phase timing logs to variant_import"
```

---

### Task 3: Verify compilation

**Files:**
- Modify: none

- [ ] **Step 1: Run cargo check**

```bash
cd src-tauri && cargo check
```

Expected: clean compile with no errors or warnings related to the new timing code.

- [ ] **Step 2: Commit (if any formatting changes needed)**

If `rustfmt` reformatted the file, stage and commit:

```bash
git add src-tauri/src/commands/variant.rs
git commit -m "style(variant): rustfmt"
```

---

### Self-Review

- **Spec coverage:** Every instrumented phase from the spec (`detect`, `copy`, `metadata`, `db_insert`, `thumbnail`, `total`) has a corresponding step.
- **Placeholder scan:** No TBD/TODO/filler steps; each step includes exact code.
- **Type consistency:** Uses `std::time::Instant` and `Instant::elapsed().as_millis()` consistently, matching `media/import.rs`.
