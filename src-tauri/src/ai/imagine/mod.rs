pub mod xai;

use async_trait::async_trait;
use serde::Serialize;
use tauri::AppHandle;

use crate::settings;

// --- Error type ---

#[derive(Debug, thiserror::Error)]
pub enum ImagineError {
    #[error("HTTP error: {0}")]
    Http(#[from] #[source] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] #[source] std::io::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("No image data in response")]
    EmptyResponse,
}

// --- Params ---

pub struct GenerateParams {
    pub prompt: String,
    pub aspect_ratio: String, // "auto" | "1:1" | "16:9" | ...
    pub resolution: String,   // "1k" | "2k"
    pub n: u32,
}

pub struct EditParams {
    pub prompt: String,
    pub image_data_url: String, // base64 data URL
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
    pub width: i32,
    pub height: i32,
    pub file_size: i64,
}

// --- Factory ---

pub fn create_provider(app: &AppHandle) -> Result<Box<dyn ImageProvider>, String> {
    let provider = settings::get_image_api_provider(app);
    match provider.as_str() {
        "xai" => {
            let api_key = settings::get_image_api_key(app);
            let base_url = settings::get_image_api_base_url(app);
            let model = settings::get_image_api_model(app);
            if api_key.is_empty() {
                return Err("xAI API key not configured".to_string());
            }
            Ok(Box::new(xai::XaiProvider::new(api_key, base_url, model)))
        }
        "" => Err("No image API provider configured".to_string()),
        _ => Err(format!("Unknown image provider: {}", provider)),
    }
}
