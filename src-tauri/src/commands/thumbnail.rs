use tauri::{command, AppHandle, Manager};

#[command]
pub fn media_thumbnail(app: AppHandle, id: String) -> Result<String, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumb_path = app_dir.join("thumbnails").join(format!("{}_256.jpg", id));

    if !thumb_path.exists() {
        return Err("Thumbnail not found".to_string());
    }

    Ok(thumb_path.to_string_lossy().replace('\\', "/"))
}
