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
    db::lineage_remove(&app, &parent_id, &child_id)
}

#[tauri::command]
pub async fn media_list_roots_count(app: AppHandle) -> Result<u64, String> {
    let path = db::db_path(&app);
    let count = db::browse_count_path(&path, &crate::media::BrowseVisibility::Representative)
        .map_err(|e| e.to_string())?;
    Ok(count as u64)
}
