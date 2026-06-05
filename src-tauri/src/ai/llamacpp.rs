use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::LazyLock;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i32>,
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

/// Shared reqwest client — reused across all HTTP calls to avoid per-request TLS
/// handshake and connection-pool churn. reqwest Client is designed for this.
static SHARED_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .expect("failed to build shared HTTP client")
});

const CAPTION_PROMPT: &str = r#"You are a professional photographer. Analyze the image and describe only information that is directly observable.

Focus on:

1. Main subject — For people: apparent age range, build, hairstyle, gender presentation, clothing, pose, expression. For objects/animals: type, condition, position.
2. Scene and environment — indoor/outdoor, setting, background elements.
3. Composition — framing, angle, rule of thirds, leading lines, symmetry.
4. Lighting conditions — direction, quality (hard/soft), source (natural/artificial), time of day cues.
5. Colors and tones — dominant palette, saturation level, warm/cool/neutral cast.
6. Camera perspective — eye-level, high angle, low angle, aerial, close-up.
7. Depth of field — shallow/deep, bokeh quality, focus plane.
8. Photography style — portrait, landscape, macro, street, documentary, snapshot.
9. Notable visual elements — text, logos, UI elements, signs. Skip if none present.

Produce your response in this format:
A dense sentence factual description. Then on a new line starting with "TAGS:", list 10 at most distinctive key objects, concepts, and visual elements as comma-separated lowercase danbooru style tags."#;

#[derive(Debug, Clone)]
pub struct SamplingParams {
    pub temperature: f32,
    pub top_p: f32,
    pub min_p: f32,
    pub repeat_penalty: f32,
    pub max_tokens: u32,
    pub seed: i32,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 0.2,
            top_p: 0.9,
            min_p: 0.05,
            repeat_penalty: 1.05,
            max_tokens: 1024,
            seed: -1, // random
        }
    }
}

pub async fn generate_caption(
    image_path: &Path,
    model: &str,
    port: u16,
    custom_prompt: Option<&str>,
    sampling: &SamplingParams,
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

    let req_body = ChatCompletionRequest {
        model: model.to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: vec![ContentPart {
                    content_type: "text".to_string(),
                    text: Some(prompt_text.to_string()),
                    image_url: None,
                }],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentPart {
                    content_type: "image_url".to_string(),
                    text: None,
                    image_url: Some(ImageUrl {
                        url: format!("data:{};base64,{}", mime, image_b64),
                    }),
                }],
            },
        ],
        stream: false,
        chat_template_kwargs: Some(serde_json::json!({"enable_thinking": false})),
        temperature: Some(sampling.temperature),
        top_p: Some(sampling.top_p),
        min_p: Some(sampling.min_p),
        repeat_penalty: Some(sampling.repeat_penalty),
        max_tokens: Some(sampling.max_tokens),
        seed: if sampling.seed >= 0 { Some(sampling.seed) } else { None },
    };

    let max_attempts = 2;
    let mut last_error = AiError::EmptyResponse;
    for attempt in 1..=max_attempts {
        let resp = SHARED_CLIENT
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
    let req_body = EmbeddingRequest {
        model: model.to_string(),
        input: text.to_string(),
    };

    let resp = SHARED_CLIENT
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
