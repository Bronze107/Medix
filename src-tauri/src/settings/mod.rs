use tauri::AppHandle;

pub const KEY_AI_MODE: &str = "ai_mode";
pub const KEY_CLOUD_PROVIDER: &str = "cloud_provider";
pub const KEY_CLOUD_API_KEY: &str = "cloud_api_key";

pub fn get(app: &AppHandle, key: &str) -> Option<String> {
    crate::db::setting_get(app, key).ok().flatten()
}

pub fn set(app: &AppHandle, key: &str, value: &str) -> Result<(), String> {
    crate::db::setting_set(app, key, value).map_err(|e| e.to_string())
}

pub fn get_ai_mode(app: &AppHandle) -> String {
    get(app, KEY_AI_MODE).unwrap_or_else(|| "auto".to_string())
}

pub fn get_cloud_provider(app: &AppHandle) -> String {
    get(app, KEY_CLOUD_PROVIDER).unwrap_or_else(|| "claude".to_string())
}

pub fn get_cloud_api_key(app: &AppHandle) -> Option<String> {
    get(app, KEY_CLOUD_API_KEY)
}
