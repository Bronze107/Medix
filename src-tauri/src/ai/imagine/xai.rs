use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{EditParams, GenerateParams, GeneratedImage, ImageProvider, ImagineError};

const MAX_API_RETRIES: u32 = 2;
const API_RETRY_BASE_MS: u64 = 2000;
const MAX_DOWNLOAD_RETRIES: u32 = 2;
const DOWNLOAD_RETRY_BASE_MS: u64 = 1000;

fn build_client(proxy: Option<&str>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(15));

    if let Some(proxy_url) = proxy {
        if let Ok(p) = reqwest::Proxy::all(proxy_url) {
            eprintln!("[imagine] using proxy {}", proxy_url);
            builder = builder.proxy(p);
        }
    }

    builder.build().expect("failed to build xAI HTTP client")
}

fn truncate_body_for_log(json: &str) -> String {
    if let Some(b64_pos) = json.find(";base64,") {
        let payload_start = b64_pos + 8;
        if let Some(quote) = json[payload_start..].find('"') {
            let mut s = String::with_capacity(payload_start + 60);
            s.push_str(&json[..payload_start]);
            s.push_str(&format!("[{}B base64]", quote));
            s.push_str(&json[payload_start + quote..]);
            return s;
        }
    }
    json.to_string()
}

/// POST JSON — only retries on connect-level errors (DNS, TCP, TLS, proxy).
/// These happen before the request body is sent, so it's safe — no double billing.
async fn post_json_with_retry<T: Serialize>(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    body: &T,
) -> Result<reqwest::Response, ImagineError> {
    let body_str = serde_json::to_string(body).unwrap_or_default();
    eprintln!("[imagine] req: {}", truncate_body_for_log(&body_str));

    let mut attempt = 0;
    loop {
        attempt += 1;
        match client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("User-Agent", "Hermes-Agent/0.14.0")
            .json(body)
            .send()
            .await
        {
            Ok(resp) => {
                eprintln!("[imagine] resp: HTTP {}", resp.status());
                return Ok(resp);
            }
            Err(e) if attempt <= MAX_API_RETRIES && e.is_connect() => {
                let delay = API_RETRY_BASE_MS * attempt as u64;
                eprintln!(
                    "[imagine] connect error, retry in {}ms (attempt {}/{})",
                    delay, attempt, MAX_API_RETRIES
                );
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            Err(e) => return Err(ImagineError::Http(e)),
        }
    }
}

/// Download a single image with retries on any network error.
/// Downloads are always safe to retry — the image is already generated.
async fn download_single(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, ImagineError> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match client
            .get(url)
            .header("User-Agent", "Hermes-Agent/0.14.0")
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return Err(ImagineError::Api(format!(
                        "download returned {}",
                        resp.status()
                    )));
                }
                return Ok(resp.bytes().await?.to_vec());
            }
            Err(e) if attempt <= MAX_DOWNLOAD_RETRIES => {
                let delay = DOWNLOAD_RETRY_BASE_MS * attempt as u64;
                eprintln!(
                    "[imagine] download error for {}, retry in {}ms (attempt {}/{})",
                    url, delay, attempt, MAX_DOWNLOAD_RETRIES
                );
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            Err(e) => return Err(ImagineError::Http(e)),
        }
    }
}

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
    client: reqwest::Client,
}

impl XaiProvider {
    pub fn new(api_key: String, base_url: String, model: String, proxy: Option<String>) -> Self {
        let client = build_client(proxy.as_deref());
        Self { api_key, base_url, model, client }
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

        let resp = post_json_with_retry(&self.client, &url, &self.api_key, &req_body).await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            eprintln!("[imagine] generate error resp ({}): {}", status, text);
            return Err(ImagineError::Api(format!("xAI generate failed ({}): {}", status, text)));
        }

        let body: ImageResponse = resp.json().await?;
        eprintln!(
            "[imagine] generate resp: {} images — {}",
            body.data.len(),
            body.data
                .iter()
                .filter_map(|d| d.url.as_deref())
                .collect::<Vec<_>>()
                .join(", ")
        );
        download_images(&self.client, &body.data).await
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

        let resp = post_json_with_retry(&self.client, &url, &self.api_key, &req_body).await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            eprintln!("[imagine] edit error resp ({}): {}", status, text);
            return Err(ImagineError::Api(format!("xAI edit failed ({}): {}", status, text)));
        }

        let body: ImageResponse = resp.json().await?;
        eprintln!(
            "[imagine] edit resp: {} images — {}",
            body.data.len(),
            body.data
                .iter()
                .filter_map(|d| d.url.as_deref())
                .collect::<Vec<_>>()
                .join(", ")
        );
        download_images(&self.client, &body.data).await
    }

    async fn health_check(&self) -> Result<bool, ImagineError> {
        // Simple auth check — try a models list or just verify key is non-empty
        if self.api_key.is_empty() {
            return Ok(false);
        }
        // Light check: try a minimal request
        let resp = self.client
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("User-Agent", "Hermes-Agent/0.14.0")
            .send()
            .await?;
        Ok(resp.status().is_success())
    }
}

async fn download_images(
    client: &reqwest::Client,
    data: &[ImageData],
) -> Result<Vec<GeneratedImage>, ImagineError> {
    let mut images = Vec::with_capacity(data.len());
    for item in data {
        if let Some(ref url) = item.url {
            match download_single(client, url).await {
                Ok(bytes) => {
                    let mime = item
                        .mime_type
                        .clone()
                        .unwrap_or_else(|| "image/png".to_string());
                    images.push(GeneratedImage {
                        mime_type: mime,
                        data: bytes,
                    });
                }
                Err(e) => {
                    eprintln!("[imagine] download failed after retries: {} — {}", url, e);
                }
            }
        }
    }
    if images.is_empty() && !data.is_empty() {
        return Err(ImagineError::EmptyResponse);
    }
    Ok(images)
}
