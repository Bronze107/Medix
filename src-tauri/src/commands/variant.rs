use std::fs;
use tauri::{command, AppHandle, Manager};

use crate::db;
use crate::variants::{list_presets, generate_variant, Variant, VariantPreset};

fn resolve_source_path(app: &AppHandle, media_id: &str) -> Result<std::path::PathBuf, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library_dir = app_dir.join("library");
    for entry in fs::read_dir(&library_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&format!("{}.", media_id)) {
            return Ok(entry.path());
        }
    }
    Err("Source file not found in library".to_string())
}

#[command]
pub fn variant_list(app: AppHandle, media_id: String) -> Result<Vec<Variant>, String> {
    db::variant_list(&app, &media_id).map_err(|e| e.to_string())
}

#[command]
pub async fn variant_generate(
    app: AppHandle,
    media_id: String,
    preset_name: String,
) -> Result<Variant, String> {
    tokio::task::spawn_blocking(move || {
        let source_path = resolve_source_path(&app, &media_id)?;
        generate_variant(&app, &media_id, &source_path, &preset_name)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[command]
pub fn variant_delete(app: AppHandle, id: String) -> Result<(), String> {
    // Delete file first
    if let Ok(Some(v)) = db::variant_get_by_id(&app, &id) {
        let _ = fs::remove_file(&v.file_path);
    }
    db::variant_delete(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn variant_presets() -> Vec<VariantPreset> {
    list_presets()
}
