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
    ];
    for key in keys {
        if let Some(val) = settings::get(&app, key) {
            map.insert(key.to_string(), val);
        }
    }
    map
}
