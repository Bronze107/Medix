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

#[derive(Debug, Serialize, Clone)]
struct ResponseFormat {
    #[serde(rename = "type")]
    r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    json_schema: Option<JsonSchema>,
}

#[derive(Debug, Serialize, Clone)]
struct JsonSchema {
    name: String,
    strict: bool,
    schema: serde_json::Value,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
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

/// Minimal JSON Schema — deliberately avoids minLength/maxLength/maxItems
/// to keep the GBNF grammar simple for small VLMs. Rust-side validation
/// handles the limits after parsing.
static CAPTION_JSON_SCHEMA: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::json!({
        "type": "object",
        "properties": {
            "caption": { "type": "string" },
            "tags": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "required": ["caption", "tags"],
        "additionalProperties": false
    })
});

static CAPTION_RESPONSE_FORMAT: LazyLock<ResponseFormat> = LazyLock::new(|| ResponseFormat {
    r#type: "json_schema".to_string(),
    json_schema: Some(JsonSchema {
        name: "caption".to_string(),
        strict: true,
        schema: CAPTION_JSON_SCHEMA.clone(),
    }),
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

Respond with a single JSON object containing exactly two fields: "caption" (a detailed paragraph of 4-6 sentences covering every observable aspect listed above) and "tags" (an array of at most 10 distinctive lowercase danbooru-style tags). Do not include markdown code blocks or any extra text outside the JSON."#;

const CAPTION_PROMPT_ZH: &str = r#"从以下方面严谨地和客观地说说这个图片是什么：

1. 主体 — 人物：大致年龄段、体型、发型、性别呈现、服装、姿势、表情。物体/动物：类型、状态、位置。
2. 场景与环境 — 室内/室外、场景类型、背景元素。
3. 构图 — 取景、角度、三分法、引导线、对称性。
4. 光线条件 — 方向、质感（硬/软）、光源（自然/人工）、时间线索。
5. 色彩与色调 — 主色调、饱和度、暖/冷/中性调。
6. 拍摄视角 — 平视、高角度、低角度、俯视、特写。
7. 景深 — 浅/深、虚化质量、焦平面。
8. 摄影风格 — 人像、风景、微距、街拍、纪实、快照。
9. 显著的视觉元素 — 文字、标志、UI 元素、标识。没有则跳过。

请只输出一个 JSON 对象，包含两个字段："caption"（一段丰富的生动的详细的中文短文描述，覆盖上述所有可观察的方面）和 "tags"（最多 10 个中文关键词的数组）。不要包含 markdown 代码块或 JSON 之外的任何额外文字。"#;

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
            max_tokens: 2048,
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
        response_format: Some(CAPTION_RESPONSE_FORMAT.clone()),
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
        println!(
            "[ai] HTTP POST /v1/chat/completions {}ms (attempt {})",
            http_ms, attempt
        );

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
            let result = parse_json_response(&text)?;
            return Ok(result);
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
    println!(
        "[ai] base64 encode {}ms (file: {})",
        b64_ms,
        image_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
    );

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
         Respond with a single JSON object containing exactly two fields: \
         \"caption\" (a dense sentence factual description) and \"tags\" \
         (an array of at most 10 distinctive lowercase danbooru-style tags). \
         Do not include markdown code blocks or any extra text outside the JSON.",
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

/// Strip an optional markdown code fence (```json ... ```) from model output.
fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }
    // Skip first fence line, e.g. ```json
    let rest = trimmed
        .find('\n')
        .map(|i| &trimmed[i + 1..])
        .unwrap_or(trimmed);
    // Remove trailing fence only if the last non-empty line is exactly ```
    let trimmed_rest = rest.trim_end_matches(|c: char| c == '\n' || c == '\r');
    if let Some(last_nl) = trimmed_rest.rfind('\n') {
        let last_line = &trimmed_rest[last_nl + 1..];
        if last_line.trim() == "```" {
            return trimmed_rest[..last_nl].trim();
        }
    } else if trimmed_rest.trim() == "```" {
        return "";
    }
    rest.trim()
}

/// Extract the first balanced `{...}` object from `text`, ignoring any
/// leading or trailing garbage (explanations, markdown, etc.).
fn extract_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in text[start..].char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..start + i + c.len_utf8()].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Trim whitespace from the inside of every JSON string value.
/// Used as a repair step when the model emits keys like `" caption"`.
fn trim_json_strings(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            let mut content = String::new();
            let mut escaped = false;
            for nc in chars.by_ref() {
                if escaped {
                    content.push(nc);
                    escaped = false;
                } else if nc == '\\' {
                    content.push(nc);
                    escaped = true;
                } else if nc == '"' {
                    break;
                } else {
                    content.push(nc);
                }
            }
            result.push('"');
            result.push_str(content.trim());
            result.push('"');
        } else {
            result.push(c);
        }
    }
    result
}

/// Try to repair common model JSON mistakes before giving up:
/// - `=` used as a key/value separator instead of `:`
/// - unquoted object keys
fn repair_json_keys(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    let mut chars = text.chars().peekable();
    // Track whether the next non-whitespace token can start a key (after `{` or `,`).
    let mut expect_key = false;

    while let Some(c) = chars.next() {
        if c == '{' || c == ',' {
            result.push(c);
            expect_key = true;
            continue;
        }

        if expect_key {
            if c.is_whitespace() {
                result.push(c);
                continue;
            }
            // Skip a spurious `=` that appears where a key should start.
            if c == '=' {
                continue;
            }
            // Quote a bare identifier key.
            if c.is_ascii_alphabetic() || c == '_' {
                let mut key = String::new();
                key.push(c);
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_alphanumeric() || n == '_' {
                        key.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek() == Some(&':') || chars.peek() == Some(&'=') {
                    result.push('"');
                    result.push_str(&key);
                    result.push('"');
                    expect_key = false;
                    continue;
                } else {
                    result.push_str(&key);
                    expect_key = false;
                    continue;
                }
            }
            // A quoted key (or `{`/`[` value) ends key-expectation.
            expect_key = false;
        }

        result.push(c);
    }
    result
}

/// Parse the model's JSON response into caption and tags.
/// Cleans tags (trim, lowercase, deduplicate), enforces length/tag-count
/// limits, and rejects empty captions.
pub(crate) fn parse_json_response(text: &str) -> Result<AiResult, AiError> {
    const MAX_CAPTION_LEN: usize = 2000;
    const MAX_TAG_LEN: usize = 100;
    const MAX_TAGS: usize = 10;

    let cleaned = strip_code_fence(text);
    let json_text = extract_json_object(cleaned).unwrap_or_else(|| cleaned.to_string());

    // Phase 1: progressively repair malformed JSON text.
    let repaired_text = repair_json_text(&json_text);

    // Phase 2: parse into a flexible Value, fix structural issues, then
    // deserialize into AiResult.
    let mut value: serde_json::Value = serde_json::from_str(&repaired_text).map_err(|e| {
        eprintln!(
            "[ai] JSON parse failed after all repairs: {}. raw text (first 500 chars): {}",
            e,
            text.chars().take(500).collect::<String>()
        );
        AiError::Json(e)
    })?;

    // Fix common structural malformations.
    let obj = value
        .as_object_mut()
        .ok_or_else(|| AiError::Server("model output is not a JSON object".to_string()))?;

    // Model sometimes outputs tags as a comma-separated string instead of an array.
    if let Some(serde_json::Value::String(s)) = obj.get("tags") {
        let tags_arr: Vec<serde_json::Value> = s
            .split(',')
            .map(|t| serde_json::Value::String(t.trim().to_string()))
            .filter(|v| v.as_str().map(|s| !s.is_empty()).unwrap_or(false))
            .collect();
        obj.insert("tags".to_string(), serde_json::Value::Array(tags_arr));
    }

    let parsed: AiResult = serde_json::from_value(value)?;

    let caption = parsed.caption.trim().to_string();
    if caption.is_empty() {
        return Err(AiError::Server("model returned empty caption".to_string()));
    }
    let caption = caption.chars().take(MAX_CAPTION_LEN).collect::<String>();

    let mut tags: Vec<String> = parsed
        .tags
        .into_iter()
        .map(|t| {
            let trimmed = t.trim().to_lowercase();
            trimmed.chars().take(MAX_TAG_LEN).collect::<String>()
        })
        .filter(|t| !t.is_empty())
        .take(MAX_TAGS)
        .collect();
    tags.sort();
    tags.dedup();
    Ok(AiResult { caption, tags })
}

/// Try progressively harsher repairs on malformed JSON text.
/// Returns the most aggressively repaired text.
fn repair_json_text(raw: &str) -> String {
    // Always apply key repair (unquote bare keys, remove spurious =, replace
    // stray single quotes) and string trimming (handles padded keys like
    // " caption"). Don't short-circuit on early "valid" JSON — keys may still
    // be wrong (e.g. " caption" is valid JSON but won't match AiResult fields).
    let trimmed = trim_json_strings(raw);
    let repaired = repair_json_keys(&trimmed).replace('\'', "\"");

    if repaired != *raw {
        eprintln!("[ai] applied JSON text repairs");
    }
    repaired
}

/// Returns the base system prompt for the given language, with an optional
/// custom prompt appended as additional instructions.
pub fn resolve_prompt(
    language: crate::settings::AiLanguage,
    custom_prompt: Option<&str>,
) -> String {
    let base = match language {
        crate::settings::AiLanguage::English => CAPTION_PROMPT,
        crate::settings::AiLanguage::Chinese => CAPTION_PROMPT_ZH,
        crate::settings::AiLanguage::Bilingual => CAPTION_PROMPT,
    };
    let mut prompt = base.to_string();
    if let Some(cp) = custom_prompt {
        prompt.push_str("\n\nAdditional user instructions:\n");
        prompt.push_str(cp);
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_response_basic() {
        let text = r#"{"caption": "a cat on a sofa", "tags": ["cat", "sofa", "indoor"]}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.caption, "a cat on a sofa");
        assert_eq!(result.tags, vec!["cat", "indoor", "sofa"]);
    }

    #[test]
    fn parse_json_response_empty_tags() {
        let text = r#"{"caption": "a simple scene", "tags": []}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.caption, "a simple scene");
        assert!(result.tags.is_empty());
    }

    #[test]
    fn parse_json_response_strips_code_fence() {
        let text = "```json\n{\"caption\": \"test\", \"tags\": [\"a\", \"b\"]}\n```";
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.caption, "test");
        assert_eq!(result.tags, vec!["a", "b"]);
    }

    #[test]
    fn parse_json_response_cleans_tags() {
        let text = r#"{"caption": "x", "tags": ["  Cat ", "CAT", "", "dog"]}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.tags, vec!["cat", "dog"]);
    }

    #[test]
    fn parse_json_response_missing_caption_fails() {
        let text = r#"{"tags": ["cat"]}"#;
        assert!(parse_json_response(text).is_err());
    }

    #[test]
    fn parse_json_response_empty_caption_fails() {
        let text = r#"{"caption": "   ", "tags": []}"#;
        assert!(parse_json_response(text).is_err());
    }

    #[test]
    fn parse_json_response_extra_properties_ignored() {
        // serde ignores unknown fields by default; json_object only enforces valid JSON.
        let text = r#"{"caption": "x", "tags": [], "extra": 1}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.caption, "x");
    }

    #[test]
    fn parse_json_response_malformed_json_fails() {
        let text = r#"{"caption": "x", "tags": [}"#;
        assert!(parse_json_response(text).is_err());
    }

    #[test]
    fn parse_json_response_missing_tags_fails() {
        let text = r#"{"caption": "x"}"#;
        assert!(parse_json_response(text).is_err());
    }

    #[test]
    fn parse_json_response_truncates_long_caption() {
        let long_caption = "x".repeat(2500);
        let text = format!(r#"{{"caption": "{}", "tags": []}}"#, long_caption);
        let result = parse_json_response(&text).unwrap();
        assert_eq!(result.caption.len(), 2000);
    }

    #[test]
    fn parse_json_response_truncates_long_tags() {
        let long_tag = "x".repeat(150);
        let text = format!(r#"{{"caption": "x", "tags": ["{}"]}}"#, long_tag);
        let result = parse_json_response(&text).unwrap();
        assert_eq!(result.tags, vec!["x".repeat(100)]);
    }

    #[test]
    fn parse_json_response_limits_tag_count() {
        let tags: Vec<String> = (0..15).map(|i| format!("tag{}", i)).collect();
        let text = format!(
            r#"{{"caption": "x", "tags": [{}]}}"#,
            tags.iter()
                .map(|t| format!("\"{}\"", t))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let result = parse_json_response(&text).unwrap();
        assert_eq!(result.tags.len(), 10);
    }

    #[test]
    fn strip_code_fence_no_language_tag() {
        let text = "```\n{\"caption\": \"x\", \"tags\": []}\n```";
        assert_eq!(strip_code_fence(text), r#"{"caption": "x", "tags": []}"#);
    }

    #[test]
    fn strip_code_fence_no_fence_returns_trimmed() {
        let text = "  {\"caption\": \"x\", \"tags\": []}  ";
        assert_eq!(strip_code_fence(text), r#"{"caption": "x", "tags": []}"#);
    }

    #[test]
    fn chat_completion_request_serializes_json_schema_response_format() {
        let req = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![],
            stream: false,
            chat_template_kwargs: None,
            temperature: None,
            top_p: None,
            min_p: None,
            repeat_penalty: None,
            max_tokens: None,
            seed: None,
            response_format: Some(CAPTION_RESPONSE_FORMAT.clone()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let rf = parsed.get("response_format").unwrap();
        assert_eq!(
            rf.get("type"),
            Some(&serde_json::Value::String("json_schema".to_string()))
        );
        let js = rf.get("json_schema").unwrap();
        assert_eq!(
            js.get("name"),
            Some(&serde_json::Value::String("caption".to_string()))
        );
        assert_eq!(js.get("strict"), Some(&serde_json::Value::Bool(true)));
        let schema = js.get("schema").unwrap();
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn extract_json_object_ignores_surrounding_text() {
        let text = "Here is the result: {\"caption\": \"x\", \"tags\": []} thanks.";
        assert_eq!(
            extract_json_object(text),
            Some(r#"{"caption": "x", "tags": []}"#.to_string())
        );
    }

    #[test]
    fn repair_json_keys_quotes_unquoted_keys() {
        assert_eq!(
            repair_json_keys(r#"{caption: "x", tags: ["a"]}"#),
            r#"{"caption": "x", "tags": ["a"]}"#
        );
    }

    #[test]
    fn repair_json_keys_removes_spurious_equals() {
        assert_eq!(
            repair_json_keys(r#"{="caption": "x", ="tags": ["a"]}"#),
            r#"{"caption": "x", "tags": ["a"]}"#
        );
    }

    #[test]
    fn parse_json_response_repairs_unquoted_keys() {
        let text = r#"{caption: "a cat", tags: ["cat", "sofa"]}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.caption, "a cat");
        assert_eq!(result.tags, vec!["cat", "sofa"]);
    }

    #[test]
    fn parse_json_response_repairs_spurious_equals() {
        let text = r#"{="caption": "a cat", ="tags": ["cat"]}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.caption, "a cat");
        assert_eq!(result.tags, vec!["cat"]);
    }

    #[test]
    fn trim_json_strings_removes_key_padding() {
        assert_eq!(
            trim_json_strings(r#"{" caption": "x", "tags ": []}"#),
            r#"{"caption": "x", "tags": []}"#
        );
    }

    #[test]
    fn parse_json_response_repairs_padded_quoted_keys() {
        // Exact reproduction of the real model output
        let text = r#"{=" caption": "city view", "tags": ["city", "sky"]}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.caption, "city view");
        assert_eq!(result.tags, vec!["city", "sky"]);
    }

    #[test]
    fn parse_json_response_converts_string_tags_to_array() {
        let text = r#"{"caption": "a cat", "tags": "cat, sofa, indoor"}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.caption, "a cat");
        assert_eq!(result.tags, vec!["cat", "indoor", "sofa"]);
    }

    #[test]
    fn parse_json_response_converts_string_tags_with_spaces() {
        let text = r#"{"caption": "x", "tags": " cat ,  dog ,fish  "}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.tags, vec!["cat", "dog", "fish"]);
    }

    #[test]
    fn parse_json_response_rejects_non_object_value() {
        let text = r#""just a string""#;
        assert!(parse_json_response(text).is_err());
    }
}
