use std::fs;
use tauri::{command, AppHandle, Manager};
use base64::Engine;

#[command]
pub fn media_thumbnail(app: AppHandle, id: String) -> Result<String, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumb_path = app_dir.join("thumbnails").join(format!("{}_256.jpg", id));

    if !thumb_path.exists() {
        return Err("Thumbnail not found".to_string());
    }

    let bytes = fs::read(&thumb_path).map_err(|e| e.to_string())?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}
