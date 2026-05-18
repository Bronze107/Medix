use tauri::{command, AppHandle};

use crate::db;
use crate::tag::Tag;

#[command]
pub fn tag_list(app: AppHandle) -> Result<Vec<Tag>, String> {
    db::tag_list(&app).map_err(|e| e.to_string())
}

#[command]
pub fn tag_create(app: AppHandle, name: String) -> Result<String, String> {
    db::tag_create(&app, &name).map_err(|e| e.to_string())
}

#[command]
pub fn tag_delete(app: AppHandle, id: String) -> Result<(), String> {
    db::tag_delete(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn tag_rename(app: AppHandle, id: String, name: String) -> Result<(), String> {
    db::tag_rename(&app, &id, &name).map_err(|e| e.to_string())
}

#[command]
pub fn media_tags_get(app: AppHandle, media_id: String) -> Result<Vec<Tag>, String> {
    db::media_tags_get(&app, &media_id).map_err(|e| e.to_string())
}

#[command]
pub fn media_tag_add(
    app: AppHandle,
    media_id: String,
    tag_id: String,
) -> Result<(), String> {
    db::media_tag_add(&app, &media_id, &tag_id).map_err(|e| e.to_string())
}

#[command]
pub fn media_tag_add_batch(
    app: AppHandle,
    media_ids: Vec<String>,
    tag_id: String,
) -> Result<(), String> {
    db::media_tag_add_batch(&app, &media_ids, &tag_id).map_err(|e| e.to_string())
}

#[command]
pub fn media_tag_remove(
    app: AppHandle,
    media_id: String,
    tag_id: String,
) -> Result<(), String> {
    db::media_tag_remove(&app, &media_id, &tag_id).map_err(|e| e.to_string())
}

#[command]
pub fn media_tag_remove_batch(
    app: AppHandle,
    media_ids: Vec<String>,
    tag_id: String,
) -> Result<(), String> {
    for id in &media_ids {
        db::media_tag_remove(&app, id, &tag_id).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[command]
pub fn media_tags_intersect(
    app: AppHandle,
    media_ids: Vec<String>,
) -> Result<Vec<Tag>, String> {
    db::media_tags_intersect(&app, &media_ids).map_err(|e| e.to_string())
}
