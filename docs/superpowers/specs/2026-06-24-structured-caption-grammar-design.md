# 强制结构化 Caption 输出设计

> 使用 llama.cpp GBNF / JSON Schema 约束 VLM 输出，替代现有自由文本 + 标记解析的 caption 生成方式。

## 背景

Medix 当前通过 system prompt 要求模型输出 `CAPTION: ... TAGS: ...` 格式的自由文本，再用正则/状态机解析。这种方案存在以下问题：

- 模型不总是严格遵守格式，偶尔输出 markdown code block、多余说明或遗漏 TAGS 标记。
- 解析器需要维护 bilingual 的 `[EN]` / `[ZH]` 状态机，代码复杂且脆弱。
- 扩展新字段（如 `confidence`、`style`）需要同时改 prompt、解析器和类型定义。

llama.cpp 已原生支持通过 `response_format.json_schema` 把 JSON Schema 转成 GBNF grammar，在采样层强制输出合法 JSON。本设计将 caption 生成改为强制 JSON 输出。

## 目标

1. 让图片/视频 AI caption 的输出格式 100% 可解析。
2. 简化 bilingual 流程：两次单语言调用，每次返回同一 JSON schema。
3. 保留 `ai_custom_prompt` 的灵活性，但输出格式仍被 grammar 约束。
4. 删除旧的文本解析代码，降低维护成本。
5. 通过 Rust 单元测试覆盖 JSON 解析和请求体构造。

## 非目标

- 不修改模型本身或 llama-server 启动参数。
- 不引入前端 UI 改动。
- 不做复杂的 tag 内容白名单校验（如只允许英文小写）。
- 不支持旧版自由文本输出回退。

## 总体方案

采用 **JSON Schema → GBNF 自动转换** 方案：

- 在 Rust 代码中内嵌固定的 JSON Schema。
- 调用 llama-server `/v1/chat/completions` 时，通过 `response_format` 字段传入 schema。
- llama.cpp 在服务端将 schema 转成 grammar 并约束采样。
- 返回内容直接 `serde_json::from_str::<AiResult>` 解析。

## 决策记录

| 决策 | 选项 | 选择 | 理由 |
|------|------|------|------|
| 输出格式 | A) JSON 对象 / B) 文本模板 / C) 可切换 | **A) JSON 对象** | 解析简单、可扩展、与 llama.cpp 原生能力对齐 |
| 迁移策略 | 彻底替换 / 开关默认开 / 开关默认关 | **彻底替换** | 代码路径单一，避免维护两套解析器 |
| 双语实现 | 一次双语调用 / 两次单语言调用 | **两次单语言调用** | 改动最小，prompt 结构不变，与现有逻辑一致 |
| 自定义 prompt | 替代 prompt / 附加指令 | **附加指令，仍强制 JSON** | 保留用户灵活性，同时保证结构化输出 |
| Grammar 来源 | JSON Schema 自动转换 / 手写 GBNF / 预生成 GBNF | **JSON Schema 自动转换** | Medix 管理 llama-server 版本，schema 简单且易维护 |

## JSON Schema 设计

```json
{
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
}
```

### 字段说明

- `caption`：单语言 caption 文本，必填，长度 1-2000。
- `tags`：标签字符串数组，可为空，最多 50 个，每个最长 100 字符。
- `additionalProperties: false`：禁止模型输出额外字段，减少幻觉。

### 双语模式

 bilingual 模式下进行两次独立调用，每次使用同一 schema：

- 英文调用 → 存 `source = "ai_en"`
- 中文调用 → 存 `source = "ai_zh"`
- tags 取自英文结果

## 请求体改造

### 新增类型

```rust
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    // ... 现有字段 ...
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    r#type: String, // "json_schema"
    json_schema: JsonSchema,
}

#[derive(Debug, Serialize)]
struct JsonSchema {
    name: String,
    schema: serde_json::Value,
    strict: bool,
}
```

### 序列化示例

```json
{
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "caption",
      "strict": true,
      "schema": { /* 见上一节 */ }
    }
  }
}
```

## Prompt 调整

在现有 system prompt 末尾统一追加：

> Respond with a single JSON object containing `caption` and `tags` fields. Do not include markdown code blocks or extra text.

中文 prompt 使用对应中文说明。

`ai_custom_prompt` 作为附加 system/user 指令追加在原 prompt 之后，grammar 仍然强制 JSON 格式。用户可以通过自定义 prompt 引导模型关注特定内容，但无法脱离 JSON schema。

## 解析器替换

### 新增函数

```rust
pub fn parse_json_response(text: &str) -> Result<AiResult, AiError> {
    // 1. 去除可选的 markdown code block 包裹
    // 2. serde_json::from_str::<AiResult>
    // 3. 清洗 tags：trim、lowercase、dedup、过滤空字符串
}
```

### 删除函数

- `parse_caption_response`
- `parse_bilingual_response`

### 清洗规则

- `caption` 取原值，仅 trim。
- `tags` 中每个元素 trim 后转小写，去重，丢弃空字符串。

## 数据流

### 图片 caption

1. `process_generate_caption` 读取 settings。
2. 根据 `ai_language` 选择：
   - English / Chinese：一次调用 `generate_caption`，结果存 `source = "ai"`。
   - Bilingual：先英文调用存 `ai_en`，再中文调用存 `ai_zh`，tags 取自英文。
3. 每次调用都在 `ChatCompletionRequest` 中附加 `response_format`。
4. `generate_caption` 内部调用 `parse_json_response` 得到 `AiResult`。
5. 存入 `captions` 表并生成 embeddings。

### 视频 caption

1. `process_video_caption` 提取帧。
2. 根据 `video_ai_multi_frame`：
   - `true`：一次 `generate_caption_multi_image` 调用，返回一个 JSON。
   - `false`：逐帧 `generate_caption`，每帧一个 JSON，最后合并。
3. bilingual 模式下同样两次调用（英文、中文）。
4. 合并逻辑与现有行为一致：caption 拼接或取最长，tags 去重合并。

## 错误处理

| 场景 | 行为 |
|------|------|
| JSON 解析失败 | 记录 error 日志（含原始输出前 200 字符），任务失败，不入库 |
| `caption` 缺失或为空 | 视为失败 |
| `tags` 缺失 | 兜底为空数组，允许入库 |
| 模型输出空内容 | 利用现有重试机制，2 次后仍空则 `AiError::EmptyResponse` |
| 自定义 prompt 导致 schema 冲突 | grammar 强制拦截，通常产生空输出或解析失败 |
| 视频单帧失败 | 记录日志，跳过该帧，继续处理其他帧 |

## 测试计划

### Rust 单元测试（`src-tauri/src/ai/`）

- `parse_json_response`：正常 JSON、空 `tags`、含 markdown code block。
- `parse_json_response`：缺失 `caption`、缺失 `tags`、非法 JSON、额外字段。
- `ChatCompletionRequest` 序列化：确认 `response_format` 字段结构正确。

### CLI 回归测试

- 若后续环境允许，新增或扩展脚本验证端到端 caption 输出。
- 由于依赖 llama-server，本次以 Rust unit test 为主。

### 手动验证

- 单张图片导入后验证 caption 和 tags。
- bilingual 模式验证 `ai_en` / `ai_zh` 两条记录。
- 视频两种多帧模式验证不崩溃。

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| llama.cpp 版本不支持 `response_format` | Medix 内置 llama-server 版本可控；文档说明最低版本要求 |
| JSON Schema 转换器 silent skip 某些约束 | schema 极简单，仅使用基本 object/string/array/required，避开已知限制 |
| 某些 VLM 对 JSON grammar 支持不佳 | 先在 prompt 中明确要求 JSON，grammar 作为强制兜底 |
| 自定义 prompt 与 grammar 冲突 | 通过测试覆盖常见自定义 prompt，并在文档中说明限制 |

## 后续可扩展

- 在 schema 中增加 `confidence` 字段评估 caption 可信度。
- 增加 `style` 字段区分描述风格（danbooru / natural language）。
- 为视频增加 `per_frame_captions` 字段，保留每帧独立描述。
