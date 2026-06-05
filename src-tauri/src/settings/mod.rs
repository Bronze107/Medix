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
pub const KEY_LLAMA_CACHE_TYPE_K: &str = "llama_cache_type_k";
pub const KEY_LLAMA_CACHE_TYPE_V: &str = "llama_cache_type_v";
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

/// Auto-detect KV cache type from model filename quantization level.
/// Falls back to explicitly configured value if set, otherwise defaults to q8_0.
fn resolve_cache_type(app: &AppHandle, key: &str) -> String {
    // If user explicitly set, use that
    if let Some(val) = get(app, key) {
        if !val.is_empty() {
            return val;
        }
    }
    // Auto-detect from model filename
    let model = get_llama_model(app);
    let filename = std::path::Path::new(&model)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if filename.contains("Q4_") {
        "q4_0".to_string()
    } else if filename.contains("Q5_") {
        "q5_0".to_string()
    } else if filename.contains("Q8_0") || filename.contains("Q8") {
        "q8_0".to_string()
    } else if filename.to_lowercase().contains("f16") {
        "f16".to_string()
    } else {
        "q8_0".to_string() // safe default
    }
}

pub fn get_llama_cache_type_k(app: &AppHandle) -> String {
    resolve_cache_type(app, KEY_LLAMA_CACHE_TYPE_K)
}

pub fn get_llama_cache_type_v(app: &AppHandle) -> String {
    resolve_cache_type(app, KEY_LLAMA_CACHE_TYPE_V)
}

pub fn get_llama_max_image_dim(app: &AppHandle) -> u32 {
    get(app, KEY_LLAMA_MAX_IMAGE_DIM)
        .and_then(|v| v.parse().ok())
        .unwrap_or(768) // 768px is sufficient for MiniCPM-V 2.6; cuts ~85% of data vs 2K
}

pub fn get_ai_custom_prompt(app: &AppHandle) -> Option<String> {
    let val = get(app, KEY_AI_CUSTOM_PROMPT)?;
    let trimmed = val.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

pub const KEY_LLAMA_TEMPERATURE: &str = "llama_temperature";
pub const KEY_LLAMA_TOP_P: &str = "llama_top_p";
pub const KEY_LLAMA_MIN_P: &str = "llama_min_p";
pub const KEY_LLAMA_REPEAT_PENALTY: &str = "llama_repeat_penalty";
pub const KEY_LLAMA_MAX_TOKENS: &str = "llama_max_tokens";
pub const KEY_LLAMA_SEED: &str = "llama_seed";

pub fn get_llama_temperature(app: &AppHandle) -> f32 {
    get(app, KEY_LLAMA_TEMPERATURE)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.2)
}

pub fn get_llama_top_p(app: &AppHandle) -> f32 {
    get(app, KEY_LLAMA_TOP_P)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.9)
}

pub fn get_llama_min_p(app: &AppHandle) -> f32 {
    get(app, KEY_LLAMA_MIN_P)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.05)
}

pub fn get_llama_repeat_penalty(app: &AppHandle) -> f32 {
    get(app, KEY_LLAMA_REPEAT_PENALTY)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.05)
}

pub fn get_llama_max_tokens(app: &AppHandle) -> u32 {
    get(app, KEY_LLAMA_MAX_TOKENS)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024)
}

pub fn get_llama_seed(app: &AppHandle) -> i32 {
    get(app, KEY_LLAMA_SEED)
        .and_then(|v| v.parse().ok())
        .unwrap_or(-1) // -1 = random
}

pub const KEY_SEMANTIC_THRESHOLD: &str = "semantic_threshold";
pub const KEY_SEARCH_SEMANTIC_ENABLED: &str = "search_semantic_enabled";
pub const KEY_SEARCH_FTS5_ENABLED: &str = "search_fts5_enabled";

pub fn get_semantic_threshold(app: &AppHandle) -> f64 {
    get(app, KEY_SEMANTIC_THRESHOLD)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.25)
}

pub fn is_semantic_search_enabled(app: &AppHandle) -> bool {
    get(app, KEY_SEARCH_SEMANTIC_ENABLED)
        .map(|v| v == "true")
        .unwrap_or(true)
}

pub fn is_fts5_search_enabled(app: &AppHandle) -> bool {
    get(app, KEY_SEARCH_FTS5_ENABLED)
        .map(|v| v == "true")
        .unwrap_or(true) // enabled by default after FTS5 implementation
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

// --- Dedicated embedding model ---

pub const KEY_EMBEDDING_MODEL: &str = "embedding_model";
pub const KEY_EMBEDDING_PORT: &str = "embedding_port";
pub const KEY_EMBEDDING_THREADS: &str = "embedding_threads";

pub fn get_embedding_model(app: &AppHandle) -> String {
    get(app, KEY_EMBEDDING_MODEL).unwrap_or_default()
}

pub fn get_embedding_port(app: &AppHandle) -> u16 {
    get(app, KEY_EMBEDDING_PORT)
        .and_then(|v| v.parse().ok())
        .unwrap_or(8081)
}

pub fn get_embedding_threads(app: &AppHandle) -> u32 {
    get(app, KEY_EMBEDDING_THREADS)
        .and_then(|v| v.parse().ok())
        .unwrap_or(2)
}

// --- Image generation API ---

pub const KEY_IMAGE_API_PROVIDER: &str = "image_api_provider";
pub const KEY_IMAGE_API_KEY: &str = "image_api_key";
pub const KEY_IMAGE_API_BASE_URL: &str = "image_api_base_url";
pub const KEY_IMAGE_API_MODEL: &str = "image_api_model";
pub const KEY_IMAGE_API_PROXY: &str = "image_api_proxy"; // legacy, migrated to global_proxy

// --- Global proxy ---

pub const KEY_GLOBAL_PROXY: &str = "global_proxy";

/// Returns the configured proxy URL, with fallback: global_proxy → legacy image_api_proxy → env vars.
pub fn get_global_proxy(app: &AppHandle) -> Option<String> {
    let configured = get(app, KEY_GLOBAL_PROXY).unwrap_or_default();
    if !configured.is_empty() {
        return Some(configured);
    }
    // Migration: fall back to old image_api_proxy key
    let old = get(app, KEY_IMAGE_API_PROXY).unwrap_or_default();
    if !old.is_empty() {
        return Some(old);
    }
    // Fall back to env vars
    std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("http_proxy"))
        .ok()
}

pub fn get_image_api_provider(app: &AppHandle) -> String {
    get(app, KEY_IMAGE_API_PROVIDER).unwrap_or_default()
}

pub fn get_image_api_key(app: &AppHandle) -> String {
    get(app, KEY_IMAGE_API_KEY).unwrap_or_default()
}

pub fn get_image_api_base_url(app: &AppHandle) -> String {
    let configured = get(app, KEY_IMAGE_API_BASE_URL).unwrap_or_default();
    if !configured.is_empty() {
        return configured;
    }
    match get_image_api_provider(app).as_str() {
        "xai" => "https://api.x.ai/v1".to_string(),
        "comfyui" => "http://localhost:8188".to_string(),
        _ => String::new(),
    }
}

pub fn get_image_api_model(app: &AppHandle) -> String {
    let configured = get(app, KEY_IMAGE_API_MODEL).unwrap_or_default();
    if !configured.is_empty() {
        return configured;
    }
    match get_image_api_provider(app).as_str() {
        "xai" => "grok-imagine-image-quality".to_string(),
        _ => String::new(),
    }
}

pub fn get_image_api_proxy(app: &AppHandle) -> Option<String> {
    get_global_proxy(app)
}
