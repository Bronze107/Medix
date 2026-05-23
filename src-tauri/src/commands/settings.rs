use std::collections::HashMap;
use tauri::{command, AppHandle};

use crate::settings;

#[command]
pub fn settings_get(app: AppHandle, key: String) -> Option<String> {
    settings::get(&app, &key)
}

#[command]
pub fn settings_set(app: AppHandle, key: String, value: String) -> Result<(), String> {
    settings::set(&app, &key, &value)
}

#[command]
pub fn settings_get_all(app: AppHandle) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let keys = vec![
        settings::KEY_AI_MODE,
        settings::KEY_CLOUD_PROVIDER,
        settings::KEY_CLOUD_API_KEY,
        settings::KEY_LLAMA_BIN_PATH,
        settings::KEY_LLAMA_PORT,
        settings::KEY_LLAMA_MODEL,
        settings::KEY_LLAMA_THREADS,
        settings::KEY_LLAMA_GPU_LAYERS,
        settings::KEY_LLAMA_CTX_SIZE,
        settings::KEY_LLAMA_MMPROJ,
        settings::KEY_LLAMA_AUTO_START,
        settings::KEY_LLAMA_MAX_IMAGE_DIM,
        settings::KEY_SEMANTIC_THRESHOLD,
        settings::KEY_THEME,
        settings::KEY_HTTP_PORT,
    ];
    for key in keys {
        if let Some(val) = settings::get(&app, key) {
            map.insert(key.to_string(), val);
        }
    }
    map
}

#[command]
pub fn saved_filters_list(
    app: AppHandle,
) -> Result<Vec<crate::db::SavedFilter>, String> {
    crate::db::saved_filters_get_all(&app).map_err(|e| e.to_string())
}

#[command]
pub fn saved_filters_save(
    app: AppHandle,
    name: String,
    query: String,
) -> Result<(), String> {
    let filter = crate::db::SavedFilter { name, query };
    crate::db::saved_filters_save(&app, &filter).map_err(|e| e.to_string())
}

#[command]
pub fn saved_filters_delete(app: AppHandle, name: String) -> Result<(), String> {
    crate::db::saved_filters_delete(&app, &name).map_err(|e| e.to_string())
}
