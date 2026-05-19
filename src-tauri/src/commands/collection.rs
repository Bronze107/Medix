use tauri::{command, AppHandle};

use crate::db::{self, Collection};

#[command]
pub fn collection_list(app: AppHandle) -> Result<Vec<Collection>, String> {
    db::collection_list(&app).map_err(|e| e.to_string())
}

#[command]
pub fn collection_get(app: AppHandle, id: String) -> Result<Option<Collection>, String> {
    db::collection_get(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn collection_create(
    app: AppHandle,
    name: String,
    description: String,
) -> Result<String, String> {
    db::collection_create(&app, &name, &description).map_err(|e| e.to_string())
}

#[command]
pub fn collection_delete(app: AppHandle, id: String) -> Result<(), String> {
    db::collection_delete(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn collection_rename(app: AppHandle, id: String, name: String) -> Result<(), String> {
    db::collection_rename(&app, &id, &name).map_err(|e| e.to_string())
}

#[command]
pub fn collection_pin(app: AppHandle, id: String) -> Result<(), String> {
    db::collection_pin(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn collection_unpin(app: AppHandle, id: String) -> Result<(), String> {
    db::collection_unpin(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn collection_add_item(
    app: AppHandle,
    collection_id: String,
    media_id: String,
) -> Result<(), String> {
    db::collection_add_item(&app, &collection_id, &media_id).map_err(|e| e.to_string())
}

#[command]
pub fn collection_add_batch(
    app: AppHandle,
    collection_id: String,
    media_ids: Vec<String>,
) -> Result<(), String> {
    db::collection_add_batch(&app, &collection_id, &media_ids).map_err(|e| e.to_string())
}

#[command]
pub fn collection_remove_item(
    app: AppHandle,
    collection_id: String,
    media_id: String,
) -> Result<(), String> {
    db::collection_remove_item(&app, &collection_id, &media_id).map_err(|e| e.to_string())
}

#[command]
pub fn media_list_by_collection(
    app: AppHandle,
    collection_id: String,
    sort_by: String,
    descending: bool,
) -> Result<Vec<crate::media::Media>, String> {
    db::media_list_by_collection(&app, &collection_id, &sort_by, descending).map_err(|e| e.to_string())
}

#[command]
pub fn collection_get_item_ids(
    app: AppHandle,
    collection_id: String,
) -> Result<Vec<String>, String> {
    db::collection_get_item_ids(&app, &collection_id).map_err(|e| e.to_string())
}

#[command]
pub fn collection_first_media_id(
    app: AppHandle,
    collection_id: String,
) -> Result<Option<String>, String> {
    db::collection_first_media_id(&app, &collection_id).map_err(|e| e.to_string())
}
