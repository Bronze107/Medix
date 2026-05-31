# AI 图像生成与编辑 — 设计文档

**日期**: 2026-05-31
**状态**: 设计阶段
**API**: xAI Grok Imagine API（`/v1/images/generations` + `/v1/images/edits`）

---

## 1. 功能范围

- **文本生图**：输入 prompt + 参数，生成全新图片
- **图像编辑**：选中已有图片，输入 prompt 进行 AI 编辑变换
- **后续扩展**：OpenAI DALL·E、本地 ComfyUI 等 provider

---

## 2. 集成策略（混合模式）

| 场景 | 集成方式 | 原因 |
|------|---------|------|
| 文本生图 | 作为**独立媒体条目**导入图库 | 无原图可依附，独立管理标签/搜索 |
| 图像编辑 | 作为原图的**新 Variant** | 天然对比，Lightbox 并排查看 |

---

## 3. 架构总览

### 3.1 目录结构

```
src-tauri/src/
├── ai/
│   ├── mod.rs              ← 已有：AI 队列 + caption
│   ├── llamacpp.rs         ← 已有：VLM + embedding
│   ├── server.rs           ← 已有：llama-server 管理
│   └── imagine/
│       ├── mod.rs          ← 新增：ImageProvider trait + 工厂函数
│       ├── xai.rs          ← 新增：XaiProvider (Grok Imagine)
│       ├── openai.rs       ← 预留：DALL·E
│       └── comfyui.rs      ← 预留：本地 ComfyUI
├── commands/
│   └── imagine.rs          ← 新增：image_generate / image_edit / confirm_import / discard_staged
├── settings/
│   └── mod.rs              ← 扩展：新增 image_api 相关 key
└── media/
    └── import.rs           ← 复用：暂存文件入正式库的导入管道

src/components/
├── AiGenPage/              ← 新增：AI 生图独立页面
├── ImagineDialog/          ← 新增：生图/编辑模态对话框（复用组件）
├── StagingPreview/         ← 新增：暂存结果审核网格
├── DetailPanel/            ← 扩展：版本标签页增加 "AI 编辑" 按钮
└── Settings/               ← 扩展：新增 "图像生成 API" 配置区
```

### 3.2 与现有系统关系

- **复用** `variants/` 系统存储编辑结果
- **复用** `library/` + 导入管道存储生图结果
- **复用** `captions` 表存储 prompt 文本（source=`ai-edit`）
- **复用** `AiQueue` 模式（spawn_blocking + mpsc channel）做异步队列
- **独立于** llama-server：图像生成是纯云端 API

---

## 4. Provider 抽象层

```rust
// src-tauri/src/ai/imagine/mod.rs

#[async_trait]
pub trait ImageProvider: Send + Sync {
    /// 文本生图，返回图片 URL 列表
    async fn generate(&self, params: &GenerateParams)
        -> Result<Vec<GeneratedImage>, ImagineError>;

    /// 图像编辑，返回图片 URL 列表
    async fn edit(&self, params: &EditParams)
        -> Result<Vec<GeneratedImage>, ImagineError>;

    /// 健康检查 / 连通性验证
    async fn health_check(&self) -> Result<bool, ImagineError>;
}

pub struct GenerateParams {
    pub prompt: String,
    pub aspect_ratio: String,  // "auto" | "1:1" | "16:9" | ...
    pub resolution: String,    // "1k" | "2k"
    pub n: u32,
    pub extra: Option<serde_json::Value>,  // provider 专属参数
}

pub struct EditParams {
    pub prompt: String,
    pub image_data_url: String,  // base64 data URL
    pub resolution: String,
    pub n: u32,
    pub extra: Option<serde_json::Value>,
}

pub struct GeneratedImage {
    pub mime_type: String,
    pub data: Vec<u8>,  // 下载的原始图片字节
}

pub fn create_provider(app: &AppHandle) -> Box<dyn ImageProvider> {
    match settings::get_image_api_provider(app).as_str() {
        "xai" => Box::new(xai::XaiProvider::new(app)),
        "openai" => Box::new(openai::OpenAiProvider::new(app)),
        "comfyui" => Box::new(comfyui::ComfyUiProvider::new(app)),
        _ => panic!("unknown image provider"),
    }
}
```

### 4.1 各 Provider 差异

| | xAI | DALL·E | ComfyUI |
|------|------|------|------|
| 生图 API | `POST /v1/images/generations` | `POST /v1/images/generations` | `POST /prompt` |
| 编辑 API | `POST /v1/images/edits` | `POST /v1/images/edits` | workflow JSON |
| `extra` 用途 | 预留 | 预留 | workflow JSON / 模板名 |
| 健康检查 | API auth 验证 | API auth 验证 | `GET /system_stats` |
| 响应格式 | `url` → 下载 | `url` → 下载 | 直接返回图片 |

### 4.2 HTTP 客户端

沿用现有 `LazyLock<reqwest::Client>` 模式，所有 provider 共享。

---

## 5. 暂存审核流程

### 5.1 核心流程

```
API → 下载图片到 staging/ → 返回 Vec<StagedImage> 给前端
  → 前端审核预览（勾选/取消勾选）
    → 确认 → confirm_import → 正式入库（library/ 或 variants/）
    → 放弃 → discard_staged → 删暂存文件
```

**暂存目录**：`%APPDATA%/com.bronze107.medix/staging/`

### 5.2 StagedImage 结构体

```rust
#[derive(Serialize)]
struct StagedImage {
    id: String,         // ULID
    temp_path: String,  // staging/ 下的文件路径
    width: i32,
    height: i32,
    file_size: i64,
}
```

### 5.3 启动清理

应用启动时自动清理 `staging/` 目录下的所有残留文件（防止上次异常退出遗留）。

---

## 6. 后端 Commands

### 6.1 文本生图

```rust
#[command]
async fn image_generate(
    app: AppHandle,
    prompt: String,
    aspect_ratio: Option<String>,  // 默认 "auto"
    resolution: Option<String>,     // 默认 "1k"
    n: Option<u32>,                // 默认 1
) -> Result<Vec<StagedImage>, String>
```

流程：
```
1. 检查 API Key 已配置
2. 调用 provider.generate(params)
3. 下载每张返回图片到 staging/{ulid}.{ext}
4. image::open 获取尺寸
5. 返回 Vec<StagedImage>
```

### 6.2 图像编辑

```rust
#[command]
async fn image_edit(
    app: AppHandle,
    media_id: String,       // 原图
    prompt: String,
    resolution: Option<String>,  // 默认 "1k"
    n: Option<u32>,         // 默认 1
) -> Result<Vec<StagedImage>, String>
```

流程：
```
1. 检查 API Key + media 存在
2. 解析原图文件路径（library/ 下）
3. 输入预处理：根据 resolution 缩放超限图片（保留原格式）
   - "1k" → 长边上限 1024px
   - "2k" → 长边上限 2048px
4. 编码为 base64 data URL（保留原 MIME 类型）
5. 请求体大小检查：>10MB 时 PNG 自动降为 JPEG Q85
6. 调用 provider.edit(params)
7. 下载 + 暂存 + 返回
```

### 6.3 确认导入

```rust
#[command]
async fn image_confirm_import(
    app: AppHandle,
    staged_ids: Vec<String>,
    prompt: String,
    // source 由后端自动从 image_api_provider 设置推导，前端不传
    media_id: Option<String>,   // 编辑模式：关联的原图 ID
) -> Result<Vec<MediaImportResult>, String>
```

流程（文本生图）：
```
staged_ids → 遍历暂存文件 → 走 import_files 管道 →
从设置读取 image_api_provider，构造 source = "generated:{provider}" →
prompt 作为 caption 存储（source="ai-generated"）
```

流程（图像编辑）：
```
staged_ids → 遍历暂存文件 → 复制到 variants/ →
从设置读取 image_api_provider，构造 source = "edited:{provider}" →
创建 Variant 记录（label=prompt前50字符）→
prompt 作为 variant caption 存储（source="ai-edit"）
```

### 6.4 丢弃暂存

```rust
#[command]
fn image_discard_staged(app: AppHandle, staged_ids: Vec<String>) -> Result<(), String>
```

删除 staging/ 下对应文件。

---

## 7. 设置键

| Key | 类型 | 默认值 | 说明 |
|-----|------|--------|------|
| `image_api_provider` | string | `""` | `"xai"` / `"openai"` / `"comfyui"` |
| `image_api_key` | string | `""` | API Key（ComfyUI 本机留空） |
| `image_api_base_url` | string | (按 provider) | xAI 默认 `https://api.x.ai/v1`，ComfyUI 默认 `http://localhost:8188`。默认值在 settings getter 中根据 provider 自动回退 |

在 `settings/mod.rs` 中的 fallback 逻辑：

```rust
pub fn get_image_api_base_url(app: &AppHandle) -> String {
    let configured = get(app, KEY_IMAGE_API_BASE_URL).unwrap_or_default();
    if !configured.is_empty() { return configured; }
    // 用户未配置时，按 provider 回退
    match get_image_api_provider(app).as_str() {
        "xai" => "https://api.x.ai/v1".to_string(),
        "comfyui" => "http://localhost:8188".to_string(),
        _ => String::new(),
    }
}
```
| `image_api_model` | string | (按 provider) | xAI 默认 `grok-imagine-image-quality` |

---

## 8. 来源追踪

### 8.1 source 字段格式

格式：`{action}:{provider}`

| 场景 | source 值 |
|------|----------|
| 文本生图 (xAI) | `generated:xai` |
| 文本生图 (DALL·E) | `generated:openai` |
| 文本生图 (ComfyUI) | `generated:comfyui` |
| 图像编辑 (xAI) | `edited:xai` |

### 8.2 前端显示

详情面板 source badge 解析格式，显示为：
```
🟢 生成:xAI     ← 现有 "生成"/"导入" badge 的扩展
🟢 编辑:ComfyUI
```

### 8.3 Prompt 记录

- **文本生图**：prompt 作为新 media 的 caption 存储，`source="ai-generated"`
- **图像编辑**：prompt 作为 variant 的 caption 存储（`caption_create_for_variant`），`source="ai-edit"`
- 详情面板正常显示 prompt，可编辑/删除

---

## 9. 前端组件

### 9.1 AI 生图页面 (`AiGenPage`)

```
┌─────────────────────────────────────────┐
│  🖼 AI 生图                              │
│  ─────────────────────────────────────── │
│  ┌────────────────────────────────────┐  │
│  │ 输入你的创意描述...                  │  │
│  └────────────────────────────────────┘  │
│  宽高比: [auto ▾]  分辨率: [1k ▾]  数量: 1│
│  [    生成图片    ]                      │
│  ─────────────────────────────────────── │
│  结果预览                                │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐  │
│  │  ✓   │ │  ✓   │ │      │ │  ✓   │  │
│  │ img1 │ │ img2 │ │ img3 │ │ img4 │  │
│  └──────┘ └──────┘ └──────┘ └──────┘  │
│  ☑ 全选  已选 3/4                       │
│  [ 放弃未选中 ]  [ 导入选中的 3 张 → ]    │
└─────────────────────────────────────────┘
```

- 侧边栏新增 `AI 生图` 导航项（与 `全部媒体`/`标签`/`集合` 同级）
- 左侧 prompt + 参数，下方审核网格
- 每张暂存图默认勾选，点击可放大
- 导入后跳转到 `全部媒体` 页面

### 9.2 AI 编辑对话框 (`ImagineDialog`)

```
┌──────────────────────────────────────┐
│  AI 图像编辑                     ✕   │
│  ─────────────────────────────────── │
│  ┌────┐                              │
│  │    │ 输入编辑指令...               │
│  └────┘ 例如："转为黑白素描风格"       │
│  生成数量: 1                          │
│  ─────────────────────────────────── │
│  结果预览（同上审核网格）              │
│  [ 取消 ]        [ 导入选中的 → ]     │
└──────────────────────────────────────┘
```

### 9.3 触发入口

| 入口 | 位置 | 场景 |
|------|------|------|
| 侧边栏导航 | `AI 生图` 菜单项 | 文本生图 |
| 版本标签页 | `AI 编辑` 按钮（与 "生成版本"/"导入版本" 并列） | 单图编辑 |
| 右键菜单 | `AI 图像编辑` 菜单项 | 批量/快捷编辑 |

### 9.4 设置页扩展

在现有 "云端 API 配置" 下方新增独立区块：

```
🖼 图像生成 API
├── 服务商: [xAI (Grok) ▾ / OpenAI (DALL·E) / ComfyUI (本地)]
├── API Key: [                         ]  ← ComfyUI 时隐藏
├── 服务地址: [                        ]  ← 选 ComfyUI 时自动 localhost:8188
└── 模型:    [                        ]  ← ComfyUI 时隐藏
```

使用与现有设置页一致的样式（`rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4`）。

---

## 10. 错误处理

| 场景 | 处理 |
|------|------|
| 未配置 API Key | 生成/编辑按钮置灰 + tooltip "请先在设置中配置 API Key" |
| API Key 无效 | Toast `图像 API 认证失败，请检查 API Key` |
| API 返回错误（余额不足等） | Toast + 错误原文 |
| 网络超时 | 重试 1 次（云端 API 不需要多次重试） |
| 图片下载失败 | 跳过该张，继续处理其他结果，Toast 告知 | 
| 磁盘空间不足 | 导入失败返回错误，不静默丢弃 |
| 请求体过大 | PNG → JPEG Q85 降级；若仍 >10MB 则报错提示 |

---

## 11. 分期计划

| 阶段 | 内容 | 预估 |
|------|------|------|
| **Phase 1** | Provider trait + XaiProvider + 设置键 + 后端 commands | 核心基础设施 |
| **Phase 2** | StagingPreview + AiGenPage + 侧边栏集成 | 文本生图完整 |
| **Phase 3** | ImagineDialog + 版本标签页 "AI 编辑" + 右键菜单 | 图像编辑完整 |
| **Phase 4** | OpenAI DALL·E provider | 预留扩展 |
| **Phase 5** | ComfyUI provider + workflow 模板 | 本地生成 |

---

## 12. 关键设计决策记录

1. **Provider 模式而非单一 API 客户端** — 为 OpenAI/ComfyUI 预留扩展点
2. **Base64 data URL 而非 Files API** — 单次请求、简单可靠、和现有 VLM 推理一致
3. **暂存 → 审核 → 确认入库** — 不自动入库，用户筛选后再导入
4. **来源细化至 provider 级别** — `generated:xai` / `edited:comfyui`
5. **Prompt 通过 caption 系统保存** — 不新增 schema，复用现有描述存储
6. **编辑输入图根据分辨率缩放** — 保留原格式、长边上限 1K/2K
7. **独立设置区而非扩充现有云配置** — 关注点分离，可同时配标注 API + 图像 API
