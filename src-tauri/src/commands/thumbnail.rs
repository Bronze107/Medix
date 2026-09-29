use serde::Serialize;
use tauri::{command, AppHandle, Manager};

#[derive(Serialize)]
pub struct ThumbnailResult {
    pub id: String,
    pub path: String,
}

#[command]
pub fn media_thumbnail(app: AppHandle, id: String) -> Result<String, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;

    let thumb_path = app_dir.join("thumbnails").join(format!("{}_256.jpg", id));

    if thumb_path.exists() {
        return Ok(thumb_path.to_string_lossy().replace('\\', "/"));
    }

    return Err("Thumbnail not found".to_string());
}

/// Batch thumbnail resolution — single IPC + single DB query instead of N.
#[command]
pub fn media_thumbnail_batch(
    app: AppHandle,
    ids: Vec<String>,
) -> Result<Vec<ThumbnailResult>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumbs_dir = app_dir.join("thumbnails");

    let mut results = Vec::with_capacity(ids.len());
    for id in &ids {
        let thumb_path = thumbs_dir.join(format!("{}_256.jpg", id));
        if thumb_path.exists() {
            results.push(ThumbnailResult {
                id: id.clone(),
                path: thumb_path.to_string_lossy().replace('\\', "/"),
            });
        }
    }

    Ok(results)
}
