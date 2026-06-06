use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter};
use ulid::Ulid;

use super::video_metadata::{self, VIDEO_EXTENSIONS};
use super::video_thumbnail;
use super::{Media, MediaImportResult};

/// Compute SHA256 of a file for dedup
fn compute_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Import a single video file
pub fn import_single_video(
    app: &AppHandle,
    source_path: &Path,
    library_dir: &Path,
) -> MediaImportResult {
    let id = Ulid::new().to_string();

    let _ = app.emit("import-progress", serde_json::json!({
        "stage": "validating",
        "current": 0,
        "total": 1,
        "filename": source_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    }));

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

    // 3. Get file size for large file check
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
    let _ = app.emit("import-progress", serde_json::json!({
        "stage": "copying",
        "current": 0,
        "total": 1,
        "filename": source_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    }));
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
    let _ = app.emit("import-progress", serde_json::json!({
        "stage": "hashing",
        "current": 0,
        "total": 1,
        "filename": source_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    }));
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
        match crate::db::media_get_by_sha256(app, hash) {
            Ok(Some(_existing)) => {
                let _ = fs::remove_file(&dest_path);
                return MediaImportResult {
                    id,
                    path: source_path.to_string_lossy().to_string(),
                    success: false,
                    error: Some("Duplicate file (SHA256 match)".to_string()),
                };
            }
            Ok(None) => {}
            Err(e) => {
                let _ = fs::remove_file(&dest_path);
                return MediaImportResult {
                    id,
                    path: source_path.to_string_lossy().to_string(),
                    success: false,
                    error: Some(format!("SHA256 lookup failed: {}", e)),
                };
            }
        }
    }

    // 6. Extract metadata
    let _ = app.emit("import-progress", serde_json::json!({
        "stage": "metadata",
        "current": 0,
        "total": 1,
        "filename": source_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    }));
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

    // 7. Generate thumbnail
    let _ = app.emit("import-progress", serde_json::json!({
        "stage": "thumbnail",
        "current": 0,
        "total": 1,
        "filename": source_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    }));
    let _thumb_result = video_thumbnail::generate_video_thumbnail(
        app,
        &id,
        &dest_path,
        metadata.duration,
    );

    // 8. Insert into database (LQIP = None for video, skip pHash)
    let _ = app.emit("import-progress", serde_json::json!({
        "stage": "database",
        "current": 0,
        "total": 1,
        "filename": source_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    }));
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
        phash: None,
        sha256,
        deleted_at: None,
        display_variant_id: None,
        thumb_256: None,
        lqip: None,
        media_type: Some("video".to_string()),
        duration: metadata.duration,
        video_codec: metadata.video_codec,
        video_fps: metadata.video_fps,
    };

    match crate::db::insert_media(app, &media) {
        Ok(_) => MediaImportResult {
            id: id.clone(),
            path: source_path.to_string_lossy().to_string(),
            success: true,
            error: None,
        },
        Err(e) => {
            let _ = fs::remove_file(&dest_path);
            MediaImportResult {
                id,
                path: source_path.to_string_lossy().to_string(),
                success: false,
                error: Some(format!("DB insert failed: {}", e)),
            }
        }
    }
}
