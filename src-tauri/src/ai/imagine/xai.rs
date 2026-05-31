use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::Duration;

use super::{EditParams, GenerateParams, GeneratedImage, ImageProvider, ImagineError};

// --- Shared HTTP client ---

static XAI_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(15));

    // Honour HTTPS_PROXY / HTTP_PROXY env vars
    if let Ok(proxy_url) = std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("http_proxy"))
    {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            builder = builder.proxy(proxy);
            eprintln!("[imagine] using proxy {}", proxy_url);
        }
    }

    builder.build().expect("failed to build xAI HTTP client")
});

// --- Request / Response types ---

#[derive(Serialize)]
struct ImageGenRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution: Option<String>,
    response_format: String,
}

#[derive(Serialize)]
struct ImageEditRequest {
    model: String,
    prompt: String,
    image: EditImageInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution: Option<String>,
    response_format: String,
}

#[derive(Serialize)]
struct EditImageInput {
    url: String,
}

#[derive(Deserialize)]
struct ImageResponse {
    data: Vec<ImageData>,
}

#[derive(Deserialize)]
struct ImageData {
    url: Option<String>,
    b64_json: Option<String>,
    mime_type: Option<String>,
}

// --- Provider ---

pub struct XaiProvider {
    api_key: String,
    base_url: String,
    model: String,
}

impl XaiProvider {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        Self { api_key, base_url, model }
    }
}

#[async_trait]
impl ImageProvider for XaiProvider {
    async fn generate(&self, params: &GenerateParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        let url = format!("{}/images/generations", self.base_url);
        eprintln!("[imagine] POST {} (model={})", url, self.model);

        let req_body = ImageGenRequest {
            model: self.model.clone(),
            prompt: params.prompt.clone(),
            n: if params.n > 1 { Some(params.n) } else { None },
            aspect_ratio: Some(params.aspect_ratio.clone()),
            resolution: Some(params.resolution.clone()),
            response_format: "url".to_string(),
        };

        let resp = XAI_CLIENT
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("User-Agent", "Hermes-Agent/0.14.0")
            .json(&req_body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ImagineError::Api(format!("xAI generate failed ({}): {}", status, text)));
        }

        let body: ImageResponse = resp.json().await?;
        download_images(&body.data).await
    }

    async fn edit(&self, params: &EditParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        let url = format!("{}/images/edits", self.base_url);
        eprintln!("[imagine] POST {} (model={})", url, self.model);

        let req_body = ImageEditRequest {
            model: self.model.clone(),
            prompt: params.prompt.clone(),
            image: EditImageInput { url: params.image_data_url.clone() },
            n: if params.n > 1 { Some(params.n) } else { None },
            resolution: Some(params.resolution.clone()),
            response_format: "url".to_string(),
        };

        let resp = XAI_CLIENT
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("User-Agent", "Hermes-Agent/0.14.0")
            .json(&req_body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ImagineError::Api(format!("xAI edit failed ({}): {}", status, text)));
        }

        let body: ImageResponse = resp.json().await?;
        download_images(&body.data).await
    }

    async fn health_check(&self) -> Result<bool, ImagineError> {
        // Simple auth check — try a models list or just verify key is non-empty
        if self.api_key.is_empty() {
            return Ok(false);
        }
        // Light check: try a minimal request
        let resp = XAI_CLIENT
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("User-Agent", "Hermes-Agent/0.14.0")
            .send()
            .await?;
        Ok(resp.status().is_success())
    }
}

async fn download_images(data: &[ImageData]) -> Result<Vec<GeneratedImage>, ImagineError> {
    let mut images = Vec::with_capacity(data.len());
    for item in data {
        if let Some(ref url) = item.url {
            let resp = XAI_CLIENT.get(url)
                .header("User-Agent", "Hermes-Agent/0.14.0")
                .send().await?;
            if !resp.status().is_success() {
                eprintln!("[imagine] failed to download image from {}", url);
                continue;
            }
            let bytes = resp.bytes().await?;
            let mime = item.mime_type.clone().unwrap_or_else(|| "image/png".to_string());
            images.push(GeneratedImage { mime_type: mime, data: bytes.to_vec() });
        }
    }
    if images.is_empty() && !data.is_empty() {
        return Err(ImagineError::EmptyResponse);
    }
    Ok(images)
}
