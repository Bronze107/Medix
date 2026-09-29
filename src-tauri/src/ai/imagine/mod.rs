pub mod comfyui;
pub mod queue;
pub mod workflow;
pub mod xai;

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Serialize;
use tauri::AppHandle;

use crate::settings;

// --- Error type ---

#[derive(Debug, thiserror::Error)]
pub enum ImagineError {
    #[error("HTTP error: {0}")]
    Http(
        #[from]
        #[source]
        reqwest::Error,
    ),
    #[error("I/O error: {0}")]
    Io(
        #[from]
        #[source]
        std::io::Error,
    ),
    #[error("API error: {0}")]
    Api(String),
    #[error("No image data in response")]
    EmptyResponse,
    #[error("WebSocket error: {0}")]
    WebSocket(String),
}

// --- Params ---

pub struct GenerateParams {
    pub prompt: String,
    pub workflow_values: HashMap<String, String>,
    pub aspect_ratio: String, // "auto" | "1:1" | "16:9" | ...
    pub resolution: String,   // "1k" | "2k"
    pub n: u32,
}

pub struct EditParams {
    pub prompt: String,
    pub workflow_values: HashMap<String, String>,
    /// 多张源图（已按目标分辨率重采样并编码为 base64 data URL）。
    pub image_data_urls: Vec<String>,
    pub aspect_ratio: String,
    pub resolution: String,
    pub n: u32,
}

pub struct GeneratedImage {
    pub mime_type: String,
    pub data: Vec<u8>,
}

// --- Trait ---

#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn generate(&self, params: &GenerateParams) -> Result<Vec<GeneratedImage>, ImagineError>;
    async fn edit(&self, params: &EditParams) -> Result<Vec<GeneratedImage>, ImagineError>;
    async fn health_check(&self) -> Result<bool, ImagineError>;
}

// --- Staging ---

#[derive(Debug, Clone, Serialize)]
pub struct StagedImage {
    pub id: String,
    pub path: String,
    pub width: i32,
    pub height: i32,
    pub file_size: i64,
}

// --- Factory ---

pub fn create_provider(
    app: &AppHandle,
    workflow_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<Box<dyn ImageProvider>, String> {
    let provider = settings::get_image_api_provider(app);
    match provider.as_str() {
        "xai" => {
            let api_key = settings::get_image_api_key(app);
            let base_url = settings::get_image_api_base_url(app);
            let model = settings::get_image_api_model(app);
            let proxy = settings::get_image_api_proxy(app);
            if api_key.is_empty() {
                return Err("xAI API key not configured".to_string());
            }
            Ok(Box::new(xai::XaiProvider::new(
                api_key, base_url, model, proxy,
            )))
        }
        "comfyui" => {
            let wf_id = workflow_id.ok_or("ComfyUI requires a workflow_id")?;
            eprintln!("[comfyui] factory: loading workflow id={}", wf_id);
            let workflow = crate::db::comfyui::comfyui_workflow_get(app, wf_id)
                .map_err(|e| format!("Workflow not found: {}", e))?;
            let base_url = settings::get_comfyui_base_url(app);
            let timeout = settings::get_comfyui_timeout_secs(app);
            eprintln!(
                "[comfyui] factory: workflow={} base_url={} timeout={}s task_id={:?}",
                workflow.name, base_url, timeout, task_id
            );
            Ok(Box::new(comfyui::ComfyuiProvider::new(
                base_url,
                timeout,
                workflow,
                app.clone(),
                task_id.unwrap_or_default().to_string(),
            )))
        }
        "" => Err("No image API provider configured".to_string()),
        _ => Err(format!("Unknown image provider: {}", provider)),
    }
}
