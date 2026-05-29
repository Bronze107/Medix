use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

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
    #[error("llama-server error: {0}")]
    Server(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No response from model")]
    EmptyResponse,
}

// --- Chat Completion (VLM) ---

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: Vec<ContentPart>,
}

#[derive(Debug, Serialize)]
struct ContentPart {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<ImageUrl>,
}

#[derive(Debug, Serialize)]
struct ImageUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: String,
}

// --- Embeddings ---

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

const CAPTION_PROMPT: &str = r#"Describe this image in detail. Then on a new line starting with "TAGS:", list key objects, concepts, and visual elements as comma-separated lowercase tags.

Example output:
A golden retriever playing fetch with a red ball in a sunny park with green grass and trees.
TAGS: dog, golden retriever, ball, park, grass, trees, outdoor, sunny"#;

pub async fn generate_caption(
    image_path: &Path,
    model: &str,
    port: u16,
    custom_prompt: Option<&str>,
) -> Result<AiResult, AiError> {
    let prompt_text = custom_prompt.unwrap_or(CAPTION_PROMPT);
    let image_bytes = tokio::fs::read(image_path).await?;
    let image_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_bytes);

    // Detect MIME type from extension
    let ext = image_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/jpeg",
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;

    let message = Message {
        role: "user".to_string(),
        content: vec![
            ContentPart {
                content_type: "image_url".to_string(),
                text: None,
                image_url: Some(ImageUrl {
                    url: format!("data:{};base64,{}", mime, image_b64),
                }),
            },
            ContentPart {
                content_type: "text".to_string(),
                text: Some(prompt_text.to_string()),
                image_url: None,
            },
        ],
    };

    let req_body = ChatCompletionRequest {
        model: model.to_string(),
        messages: vec![message],
        stream: false,
    };

    let max_attempts = 2;
    let mut last_error = AiError::EmptyResponse;
    for attempt in 1..=max_attempts {
        let resp = client
            .post(format!("http://127.0.0.1:{}/v1/chat/completions", port))
            .json(&req_body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::Server(format!("generate failed: {}", text)));
        }

        let body: ChatCompletionResponse = resp.json().await?;
        let text = body
            .choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or_default();

        if !text.is_empty() {
            let (caption, tags) = parse_caption_response(&text);
            return Ok(AiResult { caption, tags });
        }

        eprintln!(
            "[ai] empty response from model (attempt {}/{}), retrying...",
            attempt, max_attempts
        );
        tokio::time::sleep(Duration::from_secs(3)).await;
        last_error = AiError::EmptyResponse;
    }

    Err(last_error)
}

pub async fn embed_text(text: &str, model: &str, port: u16) -> Result<Vec<f32>, AiError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let req_body = EmbeddingRequest {
        model: model.to_string(),
        input: text.to_string(),
    };

    let resp = client
        .post(format!("http://127.0.0.1:{}/v1/embeddings", port))
        .json(&req_body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AiError::Server(format!("embed failed: {}", text)));
    }

    let body: EmbeddingResponse = resp.json().await?;
    body.data
        .first()
        .map(|d| d.embedding.clone())
        .ok_or(AiError::EmptyResponse)
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
