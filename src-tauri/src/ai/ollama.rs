use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

const OLLAMA_BASE: &str = "http://127.0.0.1:11434";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResult {
    pub caption: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Ollama error: {0}")]
    Ollama(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No response from model")]
    EmptyResponse,
}

#[derive(Debug, Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    images: Vec<String>,
    stream: bool,
    format: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GenerateResponse {
    response: String,
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

const CAPTION_PROMPT: &str = r#"Describe this image in detail. Then on a new line starting with "TAGS:", list key objects, concepts, and visual elements as comma-separated lowercase tags.

Example output:
A golden retriever playing fetch with a red ball in a sunny park with green grass and trees.
TAGS: dog, golden retriever, ball, park, grass, trees, outdoor, sunny"#;

pub async fn generate_caption(
    image_path: &Path,
    model: &str,
) -> Result<AiResult, AiError> {
    let image_bytes = tokio::fs::read(image_path).await?;
    let image_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_bytes);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let req_body = GenerateRequest {
        model: model.to_string(),
        prompt: CAPTION_PROMPT.to_string(),
        images: vec![image_b64],
        stream: false,
        format: serde_json::Value::Object(serde_json::Map::new()),
    };

    let resp = client
        .post(format!("{}/api/generate", OLLAMA_BASE))
        .json(&req_body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AiError::Ollama(format!("generate failed: {}", text)));
    }

    let body: GenerateResponse = resp.json().await?;
    let text = body.response.trim();
    if text.is_empty() {
        return Err(AiError::EmptyResponse);
    }

    let (caption, tags) = parse_caption_response(text);
    Ok(AiResult { caption, tags })
}

fn parse_caption_response(text: &str) -> (String, Vec<String>) {
    let mut caption = text.to_string();
    let mut tags = Vec::new();

    if let Some(idx) = text.to_uppercase().find("TAGS:") {
        caption = text[..idx].trim().to_string();
        let tags_part = &text[idx + 5..];
        tags = tags_part
            .split([',', '\n'])
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
    }

    (caption, tags)
}

pub async fn embed_text(text: &str, model: &str) -> Result<Vec<f32>, AiError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let req_body = EmbeddingRequest {
        model: model.to_string(),
        prompt: text.to_string(),
    };

    let resp = client
        .post(format!("{}/api/embeddings", OLLAMA_BASE))
        .json(&req_body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AiError::Ollama(format!("embed failed: {}", text)));
    }

    let body: EmbeddingResponse = resp.json().await?;
    Ok(body.embedding)
}
