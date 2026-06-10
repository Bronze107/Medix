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

const CAPTION_PROMPT_ZH: &str = r#"你是一名专业摄影师。分析图像并仅描述可直接观察到的信息。

关注以下方面：

1. 主体 — 人物：大致年龄段、体型、发型、性别呈现、服装、姿势、表情。物体/动物：类型、状态、位置。
2. 场景与环境 — 室内/室外、场景类型、背景元素。
3. 构图 — 取景、角度、三分法、引导线、对称性。
4. 光线条件 — 方向、质感（硬/软）、光源（自然/人工）、时间线索。
5. 色彩与色调 — 主色调、饱和度、暖/冷/中性调。
6. 拍摄视角 — 平视、高角度、低角度、俯视、特写。
7. 景深 — 浅/深、虚化质量、焦平面。
8. 摄影风格 — 人像、风景、微距、街拍、纪实、快照。
9. 显著的视觉元素 — 文字、标志、UI 元素、标识。没有则跳过。

按以下格式输出：
一段密集的中文事实描述。然后新起一行以"TAGS:"开头，列出最多10个最具特色的中文关键词，用逗号分隔。"#;

const CAPTION_PROMPT_BILINGUAL: &str = r#"You are a professional photographer. Analyze the image and describe only information that is directly observable.

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

Produce your response in this EXACT format:

[EN]
A dense sentence factual description in English.
[ZH]
一段密集的中文事实描述。
TAGS: tag1, tag2, tag3 (10 at most, comma-separated lowercase danbooru style tags)"#;

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

/// Build a base64 data URL from an image file path.
async fn image_to_data_url(image_path: &Path) -> Result<(String, u128), AiError> {
    let t = std::time::Instant::now();
    let image_bytes = tokio::fs::read(image_path).await?;
    let image_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image_bytes);
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
    let ms = t.elapsed().as_millis();
    Ok((format!("data:{};base64,{}", mime, image_b64), ms))
}

/// Shared helper: send a ChatCompletionRequest with retry and parse the response.
async fn chat_completion(
    model: &str,
    port: u16,
    messages: Vec<Message>,
    sampling: &SamplingParams,
) -> Result<AiResult, AiError> {
    let req_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        stream: false,
        chat_template_kwargs: Some(serde_json::json!({"enable_thinking": false})),
        temperature: Some(sampling.temperature),
        top_p: Some(sampling.top_p),
        min_p: Some(sampling.min_p),
        repeat_penalty: Some(sampling.repeat_penalty),
        max_tokens: Some(sampling.max_tokens),
        seed: if sampling.seed >= 0 {
            Some(sampling.seed)
        } else {
            None
        },
    };

    let max_attempts = 2;
    let mut last_error = AiError::EmptyResponse;
    for attempt in 1..=max_attempts {
        let t_req = std::time::Instant::now();
        let resp = SHARED_CLIENT
            .post(format!("http://127.0.0.1:{}/v1/chat/completions", port))
            .json(&req_body)
            .send()
            .await?;
        let http_ms = t_req.elapsed().as_millis();
        println!("[ai] HTTP POST /v1/chat/completions {}ms (attempt {})", http_ms, attempt);

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

pub async fn generate_caption(
    image_path: &Path,
    model: &str,
    port: u16,
    custom_prompt: Option<&str>,
    user_text: Option<&str>,
    sampling: &SamplingParams,
) -> Result<AiResult, AiError> {
    let prompt_text = custom_prompt.unwrap_or(CAPTION_PROMPT);
    let (data_url, b64_ms) = image_to_data_url(image_path).await?;
    println!("[ai] base64 encode {}ms (file: {})", b64_ms, image_path.file_name().and_then(|n| n.to_str()).unwrap_or("?"));

    let mut user_content: Vec<ContentPart> = Vec::new();
    if let Some(text) = user_text {
        user_content.push(ContentPart {
            content_type: "text".to_string(),
            text: Some(text.to_string()),
            image_url: None,
        });
    }
    user_content.push(ContentPart {
        content_type: "image_url".to_string(),
        text: None,
        image_url: Some(ImageUrl { url: data_url }),
    });

    let messages = vec![
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
            content: user_content,
        },
    ];

    chat_completion(model, port, messages, sampling).await
}

/// Send multiple images in a single chat completion request.
/// Frames are labelled "1/N", "2/N", etc. so the model can distinguish them.
/// All images + the prompt share one context window, enabling cross-frame
/// reasoning (supported by Qwen2-VL, InternVL2, etc.).
///
/// `user_instruction` replaces the final instruction text (defaults to
/// English if None). Pass a Chinese string for Chinese/bilingual mode.
pub async fn generate_caption_multi_image(
    image_paths: &[&Path],
    model: &str,
    port: u16,
    custom_prompt: Option<&str>,
    user_instruction: Option<&str>,
    sampling: &SamplingParams,
) -> Result<AiResult, AiError> {
    let prompt_text = custom_prompt.unwrap_or(CAPTION_PROMPT);
    let n = image_paths.len();

    // Build user message content
    let mut content_parts: Vec<ContentPart> = Vec::with_capacity(n * 2 + 1);

    for (i, image_path) in image_paths.iter().enumerate() {
        if n > 1 {
            // Language-neutral frame label (just "1/3", "2/3", etc.)
            content_parts.push(ContentPart {
                content_type: "text".to_string(),
                text: Some(format!("{}/{}", i + 1, n)),
                image_url: None,
            });
        }
        let (data_url, _b64_ms) = image_to_data_url(image_path).await?;
        content_parts.push(ContentPart {
            content_type: "image_url".to_string(),
            text: None,
            image_url: Some(ImageUrl { url: data_url }),
        });
    }

    // Final instruction (language-aware, defaults to English)
    let instruction = user_instruction.unwrap_or(
        "These frames are from the same video, in chronological order. \
         Analyze ALL frames together and produce a SINGLE response \
         (do NOT describe each frame separately). \
         Cover the video's overall content, setting, subjects, lighting, \
         colors, composition, and any notable changes, motion, or progression \
         you observe across the frames. \
         End with exactly one TAGS: line listing the most distinctive \
         tags for the video as a whole."
    );
    content_parts.push(ContentPart {
        content_type: "text".to_string(),
        text: Some(instruction.to_string()),
        image_url: None,
    });

    let messages = vec![
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
            content: content_parts,
        },
    ];

    chat_completion(model, port, messages, sampling).await
}

pub async fn embed_text(text: &str, model: &str, port: u16) -> Result<Vec<f32>, AiError> {
    let req_body = EmbeddingRequest {
        model: model.to_string(),
        input: text.to_string(),
    };

    let t = std::time::Instant::now();
    let resp = SHARED_CLIENT
        .post(format!("http://127.0.0.1:{}/v1/embeddings", port))
        .json(&req_body)
        .send()
        .await?;
    let http_ms = t.elapsed().as_millis();
    println!("[ai] embedding HTTP {}ms", http_ms);

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
    // Caption = everything before the first "TAGS:" marker (case-insensitive).
    let upper = text.to_uppercase();
    let first_tags_idx = upper.find("TAGS:");

    let caption = match first_tags_idx {
        Some(idx) => text[..idx].trim().to_string(),
        None => text.to_string(),
    };

    // Collect tags from ALL "TAGS:" lines, not just the first.
    // Multi-frame mode may produce per-frame output with multiple TAGS lines.
    let mut tags = Vec::new();
    let mut remaining = &text[first_tags_idx.unwrap_or(text.len())..];
    while let Some(idx) = remaining.to_uppercase().find("TAGS:") {
        let tags_part = &remaining[idx + 5..];
        // Take until the next "TAGS:" or end of string
        let end = tags_part.to_uppercase().find("TAGS:").unwrap_or(tags_part.len());
        let segment = &tags_part[..end];
        for tag in segment.split([',', '\n']) {
            let t = tag.trim().to_lowercase();
            if !t.is_empty() {
                tags.push(t);
            }
        }
        remaining = &tags_part[end..];
    }
    tags.sort();
    tags.dedup();

    (caption, tags)
}

/// Resolve which system prompt to use for the given language and custom prompt.
/// If a custom prompt is set, it always takes precedence.
/// Otherwise returns the appropriate built-in prompt for the language.
pub fn resolve_prompt(language: crate::settings::AiLanguage, custom_prompt: Option<&str>) -> String {
    if let Some(cp) = custom_prompt {
        return cp.to_string();
    }
    match language {
        crate::settings::AiLanguage::English => CAPTION_PROMPT.to_string(),
        crate::settings::AiLanguage::Chinese => CAPTION_PROMPT_ZH.to_string(),
        crate::settings::AiLanguage::Bilingual => CAPTION_PROMPT_BILINGUAL.to_string(),
    }
}

/// Bilingual response after parsing [EN] and [ZH] sections.
pub struct BilingualResult {
    pub caption_en: Option<String>,
    pub caption_zh: Option<String>,
    pub tags: Vec<String>,
}

/// Parse a bilingual response with [EN] / [ZH] / TAGS: sections.
/// Uses a line-based state machine to handle models that don't follow the
/// exact format (e.g. omitting newlines before section markers).
/// Falls back gracefully if markers are missing entirely.
pub fn parse_bilingual_response(text: &str) -> BilingualResult {
    eprintln!("[ai] bilingual raw response ({} chars):\n{}", text.len(), text);

    let mut caption_en = String::new();
    let mut caption_zh = String::new();
    let mut tags: Vec<String> = Vec::new();
    let mut current: Option<&str> = None;

    for line in text.lines() {
        let line_upper = line.to_uppercase();

        // Detect section switches
        if line_upper.starts_with("[EN]") || line_upper.contains("[EN]") {
            current = Some("en");
            if let Some(i) = line_upper.find("[EN]") {
                let rest = line[i + 4..].trim();
                if !rest.is_empty() {
                    caption_en.push_str(rest);
                    caption_en.push(' ');
                }
            }
            continue;
        }
        if line_upper.starts_with("[ZH]") || line_upper.contains("[ZH]") {
            current = Some("zh");
            if let Some(i) = line_upper.find("[ZH]") {
                let rest = line[i + 4..].trim();
                if !rest.is_empty() {
                    caption_zh.push_str(rest);
                    caption_zh.push(' ');
                }
            }
            continue;
        }
        // Accept "TAGS:" or "TAG:" at line start, OR "TAGS:" appearing mid-line
        // (model may put it on the same line as the ZH description)
        if line_upper.starts_with("TAGS:") || line_upper.starts_with("TAG:") {
            current = None;
            let colon = line.find(':').unwrap_or(4);
            for tag in line[colon + 1..].split(',') {
                let t = tag.trim().to_lowercase();
                if !t.is_empty() {
                    tags.push(t);
                }
            }
            continue;
        }
        // Mid-line TAGS: — extract tags and keep the text before it
        if let Some(idx) = line_upper.find("TAGS:") {
            let before = line[..idx].trim();
            if !before.is_empty() {
                match current {
                    Some("en") => { caption_en.push_str(before); caption_en.push(' '); }
                    Some("zh") => { caption_zh.push_str(before); caption_zh.push(' '); }
                    _ => {}
                }
            }
            current = None;
            for tag in line[idx + 5..].split(',') {
                let t = tag.trim().to_lowercase();
                if !t.is_empty() {
                    tags.push(t);
                }
            }
            continue;
        }

        // Accumulate text into current section
        match current {
            Some("en") => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    caption_en.push_str(trimmed);
                    caption_en.push(' ');
                }
            }
            Some("zh") => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    caption_zh.push_str(trimmed);
                    caption_zh.push(' ');
                }
            }
            _ => {}
        }
    }

    // Fallback: if no tags found yet, scan whole text for "TAGS:" or "TAG:"
    // using ASCII case-insensitive byte search (avoids Unicode case-conversion
    // byte-length changes that can misalign positions).
    if tags.is_empty() {
        let bytes = text.as_bytes();
        for i in 0..bytes.len().saturating_sub(4) {
            if bytes[i].to_ascii_uppercase() == b'T'
                && bytes[i + 1].to_ascii_uppercase() == b'A'
                && bytes[i + 2].to_ascii_uppercase() == b'G'
            {
                let colon_pos = if bytes.get(i + 3).map_or(false, |b| b.to_ascii_uppercase() == b'S')
                    && bytes.get(i + 4) == Some(&b':')
                {
                    i + 5 // "TAGS:"
                } else if bytes.get(i + 3) == Some(&b':') {
                    i + 4 // "TAG:"
                } else {
                    continue;
                };
                if text.is_char_boundary(colon_pos) {
                    eprintln!("[ai] found TAGS: marker at byte {}", i);
                    for tag in text[colon_pos..].split([',', '\n']) {
                        let t = tag.trim().to_lowercase();
                        if !t.is_empty() {
                            tags.push(t);
                        }
                    }
                    break;
                }
            }
        }
    }

    let caption_en = if caption_en.trim().is_empty() {
        None
    } else {
        Some(caption_en.trim().to_string())
    };
    let caption_zh = if caption_zh.trim().is_empty() {
        None
    } else {
        Some(caption_zh.trim().to_string())
    };

    BilingualResult { caption_en, caption_zh, tags }
}
