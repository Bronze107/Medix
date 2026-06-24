# 强制结构化 Caption 输出实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Medix 的 AI caption 生成从自由文本 + 标记解析改造为强制 JSON 输出，利用 llama-server 的 `response_format.json_schema` 在采样层约束输出格式。

**架构：** 在 `llamacpp.rs` 中内嵌 JSON Schema，通过 `ChatCompletionRequest.response_format` 传入；prompt 统一追加 JSON 输出说明；新增 `parse_json_response` 直接反序列化模型输出；删除旧的文本解析器和 bilingual 状态机；`ai/mod.rs` 的 bilingual 流程改为两次单语言调用。

**Tech Stack:** Rust, serde_json, reqwest, llama.cpp (OpenAI-compatible chat completions), Tauri

---

## 文件变更清单

| 文件 | 动作 | 说明 |
|------|------|------|
| `src-tauri/src/ai/llamacpp.rs` | 修改 | 新增 schema/response_format 类型、JSON 解析器、更新 prompt、删除旧解析器 |
| `src-tauri/src/ai/mod.rs` | 修改 | bilingual 路径把自定义 prompt 传给两次单语言调用 |
| `AGENTS.md` | 修改（可选） | 如测试策略有变化可补充说明 |

---

## Task 1: 在 `ChatCompletionRequest` 中增加 `response_format` 字段

**Files:**
- Modify: `src-tauri/src/ai/llamacpp.rs:29-48`

- [ ] **Step 1: 新增 `ResponseFormat` 和 `JsonSchema` 类型**

在 `struct ChatCompletionRequest` 之前插入：

```rust
#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    r#type: String,
    json_schema: JsonSchema,
}

#[derive(Debug, Serialize)]
struct JsonSchema {
    name: String,
    strict: bool,
    schema: serde_json::Value,
}
```

- [ ] **Step 2: 给 `ChatCompletionRequest` 增加字段**

```rust
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
```

- [ ] **Step 3: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS（仅有 unused 警告可接受）

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ai/llamacpp.rs
git commit -m "feat(ai): add response_format/json_schema types for structured caption output"
```

---

## Task 2: 内嵌 Caption JSON Schema 并提供构造 helper

**Files:**
- Modify: `src-tauri/src/ai/llamacpp.rs`

- [ ] **Step 1: 在文件顶部引入 `LazyLock`**

已有 `use std::sync::LazyLock;`，无需新增。

- [ ] **Step 2: 在 prompt 常量附近新增 schema 常量**

在 `const CAPTION_PROMPT` 之前插入：

```rust
static CAPTION_JSON_SCHEMA: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::json!({
        "type": "object",
        "properties": {
            "caption": {
                "type": "string",
                "minLength": 1,
                "maxLength": 2000
            },
            "tags": {
                "type": "array",
                "items": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 100
                },
                "maxItems": 10
            }
        },
        "required": ["caption", "tags"],
        "additionalProperties": false
    })
});

fn caption_response_format() -> ResponseFormat {
    ResponseFormat {
        r#type: "json_schema".to_string(),
        json_schema: JsonSchema {
            name: "caption".to_string(),
            strict: true,
            schema: CAPTION_JSON_SCHEMA.clone(),
        },
    }
}
```

- [ ] **Step 3: 在 `chat_completion` 请求体中附加 `response_format`**

修改 `src-tauri/src/ai/llamacpp.rs:221-236`：

```rust
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
        response_format: Some(caption_response_format()),
    };
```

- [ ] **Step 4: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/llamacpp.rs
git commit -m "feat(ai): attach caption JSON schema to chat completion requests"
```

---

## Task 3: 修改 System Prompt，要求模型输出 JSON

**Files:**
- Modify: `src-tauri/src/ai/llamacpp.rs:113-167`
- Modify: `src-tauri/src/ai/llamacpp.rs:469-481`

- [ ] **Step 1: 修改英文 prompt，把格式要求改成 JSON**

替换 `CAPTION_PROMPT` 末尾的格式说明部分（第 127-128 行）：

```rust
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

Respond with a single JSON object containing exactly two fields: "caption" (a dense sentence factual description) and "tags" (an array of at most 10 distinctive lowercase danbooru-style tags). Do not include markdown code blocks or any extra text outside the JSON."#;
```

- [ ] **Step 2: 修改中文 prompt**

替换 `CAPTION_PROMPT_ZH` 末尾的格式说明部分（第 144-145 行）：

```rust
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

请只输出一个 JSON 对象，包含两个字段："caption"（一段密集的中文事实描述）和 "tags"（最多 10 个中文关键词的数组）。不要包含 markdown 代码块或 JSON 之外的任何额外文字。"#;
```

- [ ] **Step 3: 删除 `CAPTION_PROMPT_BILINGUAL` 常量**

删除第 147-167 行的 `CAPTION_PROMPT_BILINGUAL` 定义。Bilingual 模式改为两次单语言调用，不再使用此 prompt。

- [ ] **Step 4: 修改 `resolve_prompt` 把自定义 prompt 作为附加指令**

替换 `src-tauri/src/ai/llamacpp.rs:472-481`：

```rust
pub fn resolve_prompt(language: crate::settings::AiLanguage, custom_prompt: Option<&str>) -> String {
    let base = match language {
        crate::settings::AiLanguage::English => CAPTION_PROMPT,
        crate::settings::AiLanguage::Chinese => CAPTION_PROMPT_ZH,
        crate::settings::AiLanguage::Bilingual => CAPTION_PROMPT,
    };
    let mut prompt = base.to_string();
    if language == crate::settings::AiLanguage::Bilingual {
        prompt.push_str("\n\nFor this bilingual request, return only the English caption in the JSON \"caption\" field.");
    }
    if let Some(cp) = custom_prompt {
        prompt.push_str("\n\nAdditional user instructions:\n");
        prompt.push_str(cp);
    }
    prompt
}
```

> 注：`Bilingual` 分支在此函数中仅作为兜底；`ai/mod.rs` 会直接传入 `English` / `Chinese` 各调用一次，不会真的传 `Bilingual` 进来。

- [ ] **Step 5: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS（可能有 CAPTION_PROMPT_BILINGUAL 未使用警告，下一步删除引用后消失）

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ai/llamacpp.rs
git commit -m "feat(ai): switch system prompts to JSON output format"
```

---

## Task 4: 实现 `parse_json_response` 并添加单元测试

**Files:**
- Modify: `src-tauri/src/ai/llamacpp.rs`

- [ ] **Step 1: 实现 JSON 解析函数**

在 `parse_caption_response` 函数之前（约第 422 行）插入：

```rust
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
    // Remove trailing fence if present
    if let Some(end) = rest.rfind("```") {
        rest[..end].trim()
    } else {
        rest.trim()
    }
}

/// Parse the model's JSON response into caption and tags.
/// Cleans tags (trim, lowercase, deduplicate) and rejects empty captions.
pub fn parse_json_response(text: &str) -> Result<AiResult, AiError> {
    let cleaned = strip_code_fence(text);
    let parsed: AiResult = serde_json::from_str(cleaned)?;
    let caption = parsed.caption.trim().to_string();
    if caption.is_empty() {
        return Err(AiError::Server(
            "model returned empty caption".to_string(),
        ));
    }
    let mut tags: Vec<String> = parsed
        .tags
        .into_iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    tags.sort();
    tags.dedup();
    Ok(AiResult { caption, tags })
}
```

- [ ] **Step 2: 在文件末尾添加单元测试模块**

在文件末尾（`parse_bilingual_response` 之后）添加：

```rust
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
    fn parse_json_response_extra_properties_allowed_by_default_deser() {
        // We rely on grammar to reject extra fields; serde by default ignores unknown.
        // This test documents that behavior and ensures we don't panic.
        let text = r#"{"caption": "x", "tags": [], "extra": 1}"#;
        let result = parse_json_response(text).unwrap();
        assert_eq!(result.caption, "x");
    }
}
```

- [ ] **Step 3: 运行新增测试**

Run: `cd src-tauri && cargo test --lib ai::llamacpp::tests`
Expected: 7 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ai/llamacpp.rs
git commit -m "feat(ai): add parse_json_response and unit tests"
```

---

## Task 5: 让 `chat_completion` 使用新的 JSON 解析器

**Files:**
- Modify: `src-tauri/src/ai/llamacpp.rs:262-265`

- [ ] **Step 1: 替换解析调用**

将：

```rust
        if !text.is_empty() {
            let (caption, tags) = parse_caption_response(&text);
            return Ok(AiResult { caption, tags });
        }
```

改为：

```rust
        if !text.is_empty() {
            let result = parse_json_response(&text)?;
            return Ok(result);
        }
```

- [ ] **Step 2: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ai/llamacpp.rs
git commit -m "feat(ai): wire parse_json_response into chat_completion"
```

---

## Task 6: 更新视频多帧的最终指令

**Files:**
- Modify: `src-tauri/src/ai/llamacpp.rs:361-370`

- [ ] **Step 1: 把多帧最终指令改为要求 JSON**

替换 `instruction` 字符串：

```rust
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
         Do not include markdown code blocks or any extra text outside the JSON."
    );
```

- [ ] **Step 2: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ai/llamacpp.rs
git commit -m "feat(ai): update multi-frame instruction to request JSON output"
```

---

## Task 7: 删除旧的文本解析器代码

**Files:**
- Modify: `src-tauri/src/ai/llamacpp.rs`

- [ ] **Step 1: 删除 `find_tag_marker` 函数**

删除第 425-435 行的 `find_tag_marker` 函数。

- [ ] **Step 2: 删除 `parse_caption_response` 函数**

删除第 437-467 行的 `parse_caption_response` 函数。

- [ ] **Step 3: 删除 `BilingualResult` 和 `parse_bilingual_response`**

删除第 483-626 行的 `BilingualResult` 结构体和 `parse_bilingual_response` 函数。

- [ ] **Step 4: 删除 `BilingualResult` 相关的 `pub use`**

检查 `src-tauri/src/ai/mod.rs` 的 `pub use` 行，确认没有导出 `BilingualResult` 或 `parse_bilingual_response`。当前为：

```rust
pub use llamacpp::{embed_text, generate_caption, generate_caption_multi_image, resolve_prompt, SamplingParams};
```

无需修改。

- [ ] **Step 5: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 6: 运行测试**

Run: `cd src-tauri && cargo test --lib ai::llamacpp::tests`
Expected: 7 tests PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/ai/llamacpp.rs
git commit -m "refactor(ai): remove legacy text caption parsers"
```

---

## Task 8: 更新 `ai/mod.rs` 的 bilingual 自定义 prompt 传递

**Files:**
- Modify: `src-tauri/src/ai/mod.rs:255-256`
- Modify: `src-tauri/src/ai/mod.rs:559`

- [ ] **Step 1: 图片 bilingual 路径传递自定义 prompt**

将 `src-tauri/src/ai/mod.rs:255-256`：

```rust
        let prompt_en = resolve_prompt(crate::settings::AiLanguage::English, None);
        let prompt_zh = resolve_prompt(crate::settings::AiLanguage::Chinese, None);
```

改为：

```rust
        let custom = custom_prompt.as_deref();
        let prompt_en = resolve_prompt(crate::settings::AiLanguage::English, custom);
        let prompt_zh = resolve_prompt(crate::settings::AiLanguage::Chinese, custom);
```

- [ ] **Step 2: 视频 bilingual 路径传递自定义 prompt**

将 `src-tauri/src/ai/mod.rs:559`：

```rust
            let zh_prompt = resolve_prompt(crate::settings::AiLanguage::Chinese, None);
```

改为：

```rust
            let zh_prompt = resolve_prompt(crate::settings::AiLanguage::Chinese, custom_prompt.as_deref());
```

- [ ] **Step 3: 编译检查**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ai/mod.rs
git commit -m "feat(ai): pass custom prompt to bilingual EN/ZH calls"
```

---

## Task 9: 运行全量 Rust 测试与格式检查

- [ ] **Step 1: 运行所有 Rust 单元测试**

Run: `cd src-tauri && cargo test --lib`
Expected: 现有 43 tests + 新增 7 tests 全部 PASS

- [ ] **Step 2: 运行 clippy**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: PASS（无 warning）

- [ ] **Step 3: 格式化代码**

Run: `cd src-tauri && cargo fmt`
Expected: 文件被格式化，无输出错误

- [ ] **Step 4: Commit（如 fmt 有改动）**

```bash
git add -A
git commit -m "style(ai): rustfmt"
```

---

## Task 10: 更新 AGENTS.md 测试策略（可选）

**Files:**
- Modify: `AGENTS.md`

- [ ] **Step 1: 在 AGENTS.md 的 AI 相关说明中补充 JSON 输出约束**

在 "AI 标注语言" 或 "添加 AI 模型" 附近添加一句话：

```markdown
- AI caption/tag 输出通过 llama-server `response_format.json_schema` 强制约束为 JSON（`{"caption": string, "tags": string[]}`），不再依赖 prompt 中的 `TAGS:` 标记解析。
```

- [ ] **Step 2: Commit**

```bash
git add AGENTS.md
git commit -m "docs: note structured JSON output for AI captions"
```

---

## Self-Review Checklist

- [ ] **Spec coverage**: 每个设计文档章节都有对应 task
  - JSON Schema ✅ Task 2
  - Prompt 调整 ✅ Task 3
  - 解析器替换 ✅ Task 4 + Task 5
  - 删除旧解析器 ✅ Task 7
  - 双语两次单语言调用 ✅ Task 8
  - 测试 ✅ Task 4 + Task 9
- [ ] **Placeholder scan**: 无 TBD/TODO/"实现 later"
- [ ] **类型一致性**: `ResponseFormat` / `JsonSchema` / `AiResult` 在所有 task 中名称一致
- [ ] **向后兼容**: 不修改 DB schema 或前端接口；只改 AI 输出解析方式

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-24-structured-caption-grammar-plan.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** - Dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints.

**Which approach?**
