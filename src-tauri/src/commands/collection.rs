use tauri::{command, AppHandle};

use crate::db::dataset::{DatasetItemOverride, DatasetRecipe, DatasetRecipeInput};
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
    offset: u32,
    limit: u32,
) -> Result<Vec<crate::media::Media>, String> {
    db::media_list_by_collection(&app, &collection_id, &sort_by, descending, offset, limit)
        .map_err(|e| e.to_string())
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

// --- Dataset (配方是集合的子资源；表名仍以 collection 关联) ---

#[command]
pub fn dataset_recipe_get(
    app: AppHandle,
    collection_id: String,
) -> Result<Option<DatasetRecipe>, String> {
    db::dataset::dataset_recipe_get(&app, &collection_id)
}

#[command]
pub fn dataset_recipe_save(
    app: AppHandle,
    collection_id: String,
    recipe: DatasetRecipeInput,
) -> Result<DatasetRecipe, String> {
    db::dataset::dataset_recipe_save(&app, &collection_id, &recipe)
}

#[command]
pub fn dataset_recipe_delete(app: AppHandle, collection_id: String) -> Result<(), String> {
    db::dataset::dataset_recipe_delete(&app, &collection_id)
}

#[command]
pub fn dataset_item_overrides_list(
    app: AppHandle,
    collection_id: String,
) -> Result<Vec<DatasetItemOverride>, String> {
    db::dataset::dataset_item_overrides_list(&app, &collection_id)
}

#[command]
pub fn dataset_item_override_set(
    app: AppHandle,
    collection_id: String,
    media_id: String,
    caption_mode: Option<String>,
    enabled_rules: Option<Vec<String>>,
) -> Result<(), String> {
    db::dataset::dataset_item_override_set(
        &app,
        &collection_id,
        &media_id,
        caption_mode.as_deref(),
        enabled_rules.as_deref(),
    )
}
