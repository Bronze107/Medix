use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;
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
        settings::KEY_LLAMA_CACHE_TYPE_K,
        settings::KEY_LLAMA_CACHE_TYPE_V,
        settings::KEY_LLAMA_MAX_IMAGE_DIM,
        settings::KEY_AI_CUSTOM_PROMPT,
        settings::KEY_LLAMA_TEMPERATURE,
        settings::KEY_LLAMA_TOP_P,
        settings::KEY_LLAMA_MIN_P,
        settings::KEY_LLAMA_REPEAT_PENALTY,
        settings::KEY_LLAMA_MAX_TOKENS,
        settings::KEY_LLAMA_SEED,
        settings::KEY_SEMANTIC_THRESHOLD,
        settings::KEY_SEARCH_SEMANTIC_ENABLED,
        settings::KEY_SEARCH_FTS5_ENABLED,
        settings::KEY_THEME,
        settings::KEY_HTTP_PORT,
        settings::KEY_IMAGE_API_PROVIDER,
        settings::KEY_IMAGE_API_KEY,
        settings::KEY_IMAGE_API_BASE_URL,
        settings::KEY_IMAGE_API_MODEL,
        settings::KEY_IMAGE_API_PROXY,
        settings::KEY_GLOBAL_PROXY,
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

/// Test proxy connectivity by attempting to reach xAI's API through the given proxy.
#[command]
pub fn test_proxy(proxy_url: String) -> Result<String, String> {
    let proxy = reqwest::Proxy::all(&proxy_url).map_err(|e| format!("代理地址无效: {}", e))?;
    let client = reqwest::blocking::Client::builder()
        .proxy(proxy)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建客户端失败: {}", e))?;

    match client
        .get("https://api.x.ai/v1/models")
        .header("User-Agent", "Hermes-Agent/0.14.0")
        .send()
    {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() || status.as_u16() == 401 {
                Ok(format!("连接成功 (HTTP {})", status))
            } else {
                Err(format!("代理可达但 API 返回异常 (HTTP {})", status))
            }
        }
        Err(e) => {
            let mut msg = format!("连接失败: {e}");
            let mut src = e.source();
            while let Some(inner) = src {
                msg.push_str(&format!("\n  caused by: {inner}"));
                src = inner.source();
            }
            Err(msg)
        }
    }
}
