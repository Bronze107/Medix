use std::fs;
use std::io::Read;
use std::path::Path;
use tauri::{command, AppHandle, Manager};
use ulid::Ulid;

use crate::db;
use crate::media;

/// Resolve a media item's source file path from DB or library directory.
fn resolve_source_path(app: &AppHandle, media_id: &str) -> Result<std::path::PathBuf, String> {
    // First try DB source_path
    let conn = db::get_conn(app)?;
    let db_path: Option<String> = conn
        .query_row(
            "SELECT source_path FROM media WHERE id = ?1",
            rusqlite::params![media_id],
            |r| r.get(0),
        )
        .ok();
    if let Some(p) = db_path {
        if !p.is_empty() {
            let path = Path::new(&p);
            if path.exists() {
                return Ok(path.to_path_buf());
            }
        }
    }
    // Fallback: scan library directory
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library_dir = app_dir.join("library");
    for entry in fs::read_dir(&library_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&format!("{}.", media_id)) {
            return Ok(entry.path());
        }
    }
    Err("Source file not found".to_string())
}

#[command]
pub async fn media_generate_derivative(
    app: AppHandle,
    source_media_id: String,
    label: String,
    format: String,
    max_width: Option<u32>,
    max_height: Option<u32>,
    quality: u8,
    resize_filter: Option<String>,
) -> Result<media::Media, String> {
    tokio::task::spawn_blocking(move || {
        let source_path = resolve_source_path(&app, &source_media_id)?;
        let result = media::transform::generate_derivative(
            &app,
            &source_media_id,
            &source_path,
            &label,
            &format,
            max_width,
            max_height,
            quality,
            resize_filter.as_deref(),
        )
        .map_err(|e| e.to_string())?;

        // Generate thumbnail from the new file
        if let Some(ref path) = result.source_path {
            let thumb_path = Path::new(path);
            if let Err(e) = media::thumbnail::generate_thumbnails(&app, &result.id, thumb_path) {
                eprintln!("[transform] thumbnail failed for {}: {}", result.id, e);
            }
        }

        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[command]
pub fn media_import_derivative(
    app: AppHandle,
    source_media_id: String,
    file_path: String,
) -> Result<media::Media, String> {
    let src = Path::new(&file_path);
    if !src.exists() {
        return Err("Source file not found".to_string());
    }

    // Detect format from magic bytes
    let mut first_bytes = vec![0u8; 12];
    let mut f = fs::File::open(src).map_err(|e| format!("Failed to open: {}", e))?;
    let n = f.read(&mut first_bytes).unwrap_or(0);
    drop(f);

    let ext = if let Some(detected) = media::import::detect_format_from_bytes(&first_bytes[..n]) {
        detected.to_string()
    } else {
        src.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default()
    };

    if ext.is_empty() {
        return Err("Could not determine file format".to_string());
    }

    let file_size = src.metadata().map_err(|e| e.to_string())?.len() as i64;

    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library_dir = app_dir.join("library");
    fs::create_dir_all(&library_dir).map_err(|e| e.to_string())?;

    let id = Ulid::new().to_string();
    let file_name = format!("{}.{}", id, ext);
    let dest = library_dir.join(&file_name);
    fs::copy(src, &dest).map_err(|e| e.to_string())?;

    // Decode for dimensions
    let data = fs::read(&dest).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&data)
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    let (width, height) = (img.width() as i32, img.height() as i32);

    let dest_str = dest.to_string_lossy().replace('\\', "/");
    let now = chrono::Utc::now().to_rfc3339();

    let media = media::Media {
        id: id.clone(),
        source_path: Some(dest_str),
        width: Some(width),
        height: Some(height),
        file_size: Some(file_size),
        created_at: None,
        modified_at: None,
        imported_at: now,
        source_url: None,
        page_url: None,
        source: Some("imported".into()),
        phash: None,
        sha256: None,
        deleted_at: None,
        display_variant_id: None,
        thumb_256: None,
        lqip: None,
        media_type: Some("image".into()),
        duration: None,
        video_codec: None,
        video_fps: None,
    };

    db::insert_media(&app, &media).map_err(|e| e.to_string())?;
    db::lineage_insert(&app, &source_media_id, &id, "import", None)
        .map_err(|e| e.to_string())?;

    // Generate thumbnail
    if let Err(e) = media::thumbnail::generate_thumbnails(&app, &id, &dest) {
        eprintln!("[transform] thumbnail failed for imported {}: {}", id, e);
    }

    Ok(media)
}
