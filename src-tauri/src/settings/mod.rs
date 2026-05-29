use tauri::AppHandle;

// --- AI mode ---
pub const KEY_AI_MODE: &str = "ai_mode";
pub const KEY_CLOUD_PROVIDER: &str = "cloud_provider";
pub const KEY_CLOUD_API_KEY: &str = "cloud_api_key";

// --- llama.cpp ---
pub const KEY_LLAMA_BIN_PATH: &str = "llama_bin_path";
pub const KEY_LLAMA_PORT: &str = "llama_port";
pub const KEY_LLAMA_MODEL: &str = "llama_model";
pub const KEY_LLAMA_THREADS: &str = "llama_threads";
pub const KEY_LLAMA_GPU_LAYERS: &str = "llama_gpu_layers";
pub const KEY_LLAMA_CTX_SIZE: &str = "llama_ctx_size";
pub const KEY_LLAMA_MMPROJ: &str = "llama_mmproj";
pub const KEY_LLAMA_AUTO_START: &str = "llama_auto_start";
pub const KEY_LLAMA_MAX_IMAGE_DIM: &str = "llama_max_image_dim";
pub const KEY_AI_CUSTOM_PROMPT: &str = "ai_custom_prompt";

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

// --- llama.cpp getters with defaults ---

pub fn get_llama_bin_path(app: &AppHandle) -> String {
    get(app, KEY_LLAMA_BIN_PATH).unwrap_or_else(|| "llama-server".to_string())
}

pub fn get_llama_port(app: &AppHandle) -> u16 {
    get(app, KEY_LLAMA_PORT)
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080)
}

pub fn get_llama_model(app: &AppHandle) -> String {
    get(app, KEY_LLAMA_MODEL).unwrap_or_default()
}

pub fn get_llama_threads(app: &AppHandle) -> u32 {
    get(app, KEY_LLAMA_THREADS)
        .and_then(|v| v.parse().ok())
        .unwrap_or(4)
}

pub fn get_llama_gpu_layers(app: &AppHandle) -> i32 {
    get(app, KEY_LLAMA_GPU_LAYERS)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

pub fn get_llama_ctx_size(app: &AppHandle) -> u32 {
    get(app, KEY_LLAMA_CTX_SIZE)
        .and_then(|v| v.parse().ok())
        .unwrap_or(4096)
}

pub fn get_llama_mmproj(app: &AppHandle) -> String {
    get(app, KEY_LLAMA_MMPROJ).unwrap_or_default()
}

pub fn get_llama_auto_start(app: &AppHandle) -> bool {
    get(app, KEY_LLAMA_AUTO_START)
        .map(|v| v == "true")
        .unwrap_or(false)
}

pub fn get_llama_max_image_dim(app: &AppHandle) -> u32 {
    get(app, KEY_LLAMA_MAX_IMAGE_DIM)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0) // 0 means no resize
}

pub fn get_ai_custom_prompt(app: &AppHandle) -> Option<String> {
    let val = get(app, KEY_AI_CUSTOM_PROMPT)?;
    let trimmed = val.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

pub const KEY_SEMANTIC_THRESHOLD: &str = "semantic_threshold";

pub fn get_semantic_threshold(app: &AppHandle) -> f64 {
    get(app, KEY_SEMANTIC_THRESHOLD)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.25)
}

pub const KEY_THEME: &str = "theme";

pub fn get_theme(app: &AppHandle) -> String {
    get(app, KEY_THEME).unwrap_or_else(|| "dark".to_string())
}

pub const KEY_HTTP_PORT: &str = "http_port";

pub fn get_http_port(app: &AppHandle) -> u16 {
    get(app, KEY_HTTP_PORT)
        .and_then(|v| v.parse().ok())
        .unwrap_or(8765)
}
