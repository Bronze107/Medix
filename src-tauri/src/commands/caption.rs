use tauri::{command, AppHandle};

use crate::captions::Caption;
use crate::db;

#[command]
pub fn caption_list(app: AppHandle, media_id: String) -> Result<Vec<Caption>, String> {
    db::caption_list(&app, &media_id).map_err(|e| e.to_string())
}

#[command]
pub fn caption_create(app: AppHandle, media_id: String, text: String) -> Result<Caption, String> {
    db::caption_create(&app, &media_id, &text).map_err(|e| e.to_string())
}

#[command]
pub fn caption_update(app: AppHandle, id: String, text: String) -> Result<(), String> {
    db::caption_update(&app, &id, &text).map_err(|e| e.to_string())
}

#[command]
pub fn caption_delete(app: AppHandle, id: String) -> Result<(), String> {
    db::caption_delete(&app, &id).map_err(|e| e.to_string())
}
