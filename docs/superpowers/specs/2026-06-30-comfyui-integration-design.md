# ComfyUI Provider Integration Design

> **Goal:** Add ComfyUI as a local image generation provider alongside xAI, reusing the existing ImageProvider trait, ImageQueue, staging, and import lifecycle.

**Architecture:** New `ComfyuiProvider` implementing `ImageProvider` trait. Named workflow management (CRUD) similar to Variant Presets. Workflow JSON nodes with `#param` title convention expose configurable parameters to the frontend as dynamic forms.

**Tech Stack:** Rust (reqwest HTTP client, serde_json), TypeScript/React (dynamic form rendering), SQLite (workflow storage)

---

## 1. Workflow `#` Parameter Convention

Users mark nodes they want Medix to expose by editing the node title in ComfyUI with a `#` prefix:

```
#param_name[=default][:type]
```

| Format | Description | Renders as |
|--------|-------------|------------|
| `#prompt` | Simplest form | multiline textarea (auto-detected from `CLIPTextEncode`) |
| `#negative_prompt=low quality` | With default value | text input |
| `#steps=20` | With default | slider |
| `#seed=-1:seed` | `:seed` type hint | number input + randomize button |
| `#cfg=7:slider` | `:slider` with default | slider (range from widget info) |
| `#input_image` | (edit only) | image selector bound to current media |
| `#width=1024` / `#height=1024` | Resolution params | resolution preset picker |

### Auto-inference

Even without a type suffix, Medix infers the form control from the node's class_type:

| class_type | Widget | Field type |
|------------|--------|------------|
| `CLIPTextEncode` | text | `multiline` |
| `EmptyLatentImage` | width, height | `number` (with resolution preset) |
| `KSampler` | seed | `seed` |
| `KSampler` | steps, cfg, denoise | `slider` |
| `LoadImage` | image | `image_selector` |

Non-`#` nodes keep their workflow JSON values unchanged.

### Validation

- Workflow JSON must contain at least one `#`-prefixed node title
- Workflow JSON must be valid JSON with `nodes` array (ComfyUI format)
- Duplicate `#param_name` across nodes is an error at save time

---

## 2. Data Model

### Rust

```rust
// ai/imagine/workflow.rs

pub struct ComfyWorkflow {
    pub id: String,            // ULID
    pub name: String,
    pub workflow_type: String, // "generate" | "edit"
    pub workflow_json: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct WorkflowParam {
    pub node_id: String,
    pub param_name: String,    // without # prefix
    pub widget_name: String,   // the actual widget field name on the node
    pub default_value: String,
    pub field_type: String,    // "text"|"multiline"|"number"|"slider"|"seed"|"image_selector"
    pub order_index: usize,
}
```

### SQLite

```sql
CREATE TABLE comfyui_workflows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    workflow_type TEXT NOT NULL DEFAULT 'generate',
    workflow_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### TypeScript

```typescript
// src/types/comfyui.ts

export interface ComfyWorkflow {
  id: string;
  name: string;
  workflow_type: "generate" | "edit";
  workflow_json: string;
  created_at: string;
  updated_at: string;
}

export interface WorkflowParam {
  node_id: string;
  param_name: string;
  widget_name: string;
  default_value: string;
  field_type: "text" | "multiline" | "number" | "slider" | "seed" | "image_selector";
  order_index: number;
}
```

---

## 3. Tauri Commands

| Command | Signature | Notes |
|---------|-----------|-------|
| `comfyui_workflow_list` | `(workflow_type?: string) -> Vec<ComfyWorkflow>` | List all, filterable by type |
| `comfyui_workflow_get` | `(id: string) -> ComfyWorkflow` | With parsed `WorkflowParam[]` |
| `comfyui_workflow_create` | `(name, type, json) -> ComfyWorkflow` | Parses and validates `#` nodes on save |
| `comfyui_workflow_update` | `(id, name, json) -> ComfyWorkflow` | Re-parses + updates |
| `comfyui_workflow_delete` | `(id) -> void` | |
| `comfyui_test_connection` | `() -> String` | GET /system_stats, returns ComfyUI version or error |

### Settings

| Key | Default | Notes |
|-----|---------|-------|
| `image_api_provider` | (existing) | New accepted value: `"comfyui"` |
| `comfyui_base_url` | `"http://127.0.0.1:8188"` | |
| `comfyui_timeout_secs` | `"300"` | Per-task timeout |

---

## 4. ComfyuiProvider: ImageProvider Implementation

### File: `src-tauri/src/ai/imagine/comfyui.rs`

```rust
pub struct ComfyuiProvider {
    base_url: String,
    timeout_secs: u64,
    workflow: ComfyWorkflow,  // pre-loaded workflow with parsed params
}

#[async_trait]
impl ImageProvider for ComfyuiProvider {
    async fn generate(&self, params: &GenerateParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        // 1. Clone workflow JSON
        // 2. For each WorkflowParam, inject user-provided values into matching node
        // 3. POST /prompt with modified workflow
        // 4. Poll GET /history/{prompt_id} until complete
        // 5. For each output image: GET /view?filename=xxx, write to staging dir
        // 6. Return Vec<GeneratedImage>
    }

    async fn edit(&self, params: &EditParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        // Same as generate, plus:
        // - POST /upload/image with base64 image data
        // - Inject returned filename into #input_image node
    }

    async fn health_check(&self) -> Result<bool, ImagineError> {
        // GET /system_stats → Ok(true) on 200
    }
}
```

### ComfyUI Protocol

| Step | Method | Request | Response |
|------|--------|---------|----------|
| Submit | `POST /prompt` | `{"prompt": <workflow_json>, "client_id": "medix"}` | `{"prompt_id": "..."}` |
| Poll | `GET /history/{prompt_id}` | - | `{"outputs": {"<node_id>": {"images": [{"filename": "...", "subfolder": "...", "type": "output"}]}}}` |
| Download | `GET /view?filename=...&subfolder=...&type=output` | - | Binary image data |
| Health | `GET /system_stats` | - | `{"system": {...}}` |
| Upload | `POST /upload/image` | Multipart form with image file | `{"name": "..."}` |

### Parameter Injection Logic (WorkflowManager)

```
Frontend form values:
  prompt = "a cat"
  steps = 20
  seed = 42

WorkflowManager.inject(workflow_json, params_map):
  For each node in workflow["nodes"]:
    title = node["_meta"]["title"]
    if title starts with "#":
      param_name = extract from title
      widget_name = determine widget field
      new_value = params_map[param_name]
      node["widgets_values"][widget_index] = new_value  // or node.inputs.*
  Return modified workflow_json
```

### Polling

```rust
// Poll with linear backoff
loop {
    let elapsed = start.elapsed();
    if elapsed > timeout { return Err(ImagineError::Timeout); }

    let resp = client.get(&format!("{}/history/{}", base_url, prompt_id)).send().await?;
    if resp.status() == 200 {
        break; // Parse outputs
    }
    // 404 or {} means still processing
    tokio::time::sleep(Duration::from_secs(2)).await;
}
```

### Image Download (Edit mode)

For edit mode, the source image must be uploaded to ComfyUI:
1. Read media file from library (or specific variant)
2. `POST /upload/image` as multipart
3. Inject returned filename into `#input_image` node's widget

### Error Handling

| Scenario | Error variant |
|----------|--------------|
| Connection refused | `ImagineError::Connection("ComfyUI not running at {url}")` |
| Timeout | `ImagineError::Timeout("Task exceeded {n}s")` |
| No output images | `ImagineError::EmptyResponse("No images in ComfyUI output")` |
| Invalid workflow JSON | Caught at save time, not at submission time |

---

### workflow_id Through the Queue

The submit commands gain an optional `workflow_id` parameter (required when provider is "comfyui", ignored otherwise):

```rust
// queue.rs — ImageTask enum extended
pub enum ImageTask {
    Generate { task_id, prompt, aspect_ratio, resolution, n, workflow_id: Option<String> },
    Edit { task_id, media_id, variant_id, prompt, aspect_ratio, resolution, n, workflow_id: Option<String> },
}
```

```typescript
// tauri.ts — updated wrappers
imageQueueSubmitGenerate(prompt, aspectRatio?, resolution?, n?, workflowId?) -> Promise<string>
imageQueueSubmitEdit(mediaId, variantId?, prompt, aspectRatio?, resolution?, n?, workflowId?) -> Promise<string>
```

The workflow_id is stored in `TaskState` and passed to `create_provider(app, workflow_id)` at processing time.

---

## 5. Factory Update

### File: `src-tauri/src/ai/imagine/mod.rs`

Extend `create_provider()` to handle `"comfyui"`:

```rust
pub fn create_provider(app: &AppHandle, workflow_id: &str) -> Result<Box<dyn ImageProvider>, String> {
    let provider_type = get_image_api_provider(app);
    match provider_type.as_str() {
        "xai" => { /* existing */ }
        "comfyui" => {
            let workflow = db::comfyui_workflow_get(app, workflow_id)?;
            let base_url = settings::get(app, "comfyui_base_url")
                .unwrap_or_else(|| "http://127.0.0.1:8188".into());
            let timeout = settings::get(app, "comfyui_timeout_secs")
                .and_then(|s| s.parse().ok())
                .unwrap_or(300);
            Ok(Box::new(ComfyuiProvider::new(base_url, timeout, workflow)))
        }
        _ => Err("Unknown image API provider".into()),
    }
}
```

---

## 6. Frontend

### Settings Page — ComfyUI Section

When `image_api_provider = "comfyui"`:

```
┌─ ComfyUI 配置 ─────────────────────────────┐
│ 地址          [http://127.0.0.1:8188      ] │
│ 超时(秒)      [300                   ]       │
│              [测试连接]                       │
├─────────────────────────────────────────────┤
│ 工作流                                       │
│ 文生图                                       │
│ ┌─ SDXL写实人像 ────────── [编辑] [删除] ─┐ │
│ ┌─ Flux动漫风 ──────────── [编辑] [删除] ─┐ │
│ [+ 添加文生图工作流]                         │
│ 图生图                                       │
│ ┌─ Img2Img通用 ────────── [编辑] [删除] ──┐ │
│ [+ 添加图生图工作流]                         │
└─────────────────────────────────────────────┘
```

Workflow create/edit dialog:
- Name text input
- Large textarea for workflow JSON
- Save button (validates: JSON parseable, at least one `#` node)

### AiGenPage — Dynamic Form

When provider is ComfyUI:

```
Provider       [ComfyUI ▼]
工作流          [SDXL写实人像 ▼]    ← selects workflow_id

── workflow-specific params ──

#prompt         ┌──────────────────────┐
                │ a cat wearing        │
                │ sunglasses           │
                └──────────────────────┘

#negative_prompt  [low quality, blurry          ]

#steps             [══════●═══════] 20

#seed              [42] [🎲]

#cfg               [══════●═══════] 7.0

#width × #height   [1024×1024 1:1 ▼]

── ────────────────────────── ──

[生成 1 张]
```

Renders existing TaskCard list with results, same import/discard/dismiss lifecycle as xAI.

### ImagineDialog — Edit Mode

Same dynamic form pattern, workflow list filtered to `workflow_type = "edit"`. `image_selector` fields are bound to the current media image (read-only display of the source).

---

## 7. File Structure

```
Create:
  src-tauri/src/ai/imagine/comfyui.rs    — ComfyuiProvider + WorkflowManager
  src-tauri/src/commands/comfyui.rs      — Tauri commands (workflow CRUD)
  src-tauri/src/db/comfyui.rs            — DB CRUD for comfyui_workflows table
  src/types/comfyui.ts                   — Frontend types
  src/components/Settings/ComfyuiSettings.tsx   — Settings UI section
  src/components/AiGenPage/WorkflowForm.tsx     — Dynamic form based on #params

Modify:
  src-tauri/src/ai/imagine/mod.rs        — Factory: add "comfyui" branch
  src-tauri/src/ai/imagine/queue.rs      — Pass workflow_id through task
  src-tauri/src/db/mod.rs                — Migration for comfyui_workflows table
  src-tauri/src/settings/mod.rs          — Add comfyui_base_url, comfyui_timeout_secs
  src-tauri/src/main.rs                  — Register new commands
  src/lib/tauri.ts                       — Add type-safe wrappers
  src/components/AiGenPage/AiGenPage.tsx — Select workflow, render dynamic form
  src/components/Settings/Settings.tsx   — Wire ComfyUI section
```

---

## 8. Testing

### CLI Regression Test: `tests/comfyui.sh`

- Set provider to "comfyui"
- Create workflow via CLI `exec` SQL directly (bypasses UI)
- List workflows, verify parsed `#` params
- Update and delete workflows
- Test connection (mock or skip if no ComfyUI running)

### Rust Unit Tests

- `WorkflowManager::parse_params()` — parse `#param=default:type` from mock workflow JSON
- `WorkflowManager::inject()` — verify correct widget values in output JSON
- `ComfyuiProvider` request construction (with mocked HTTP client)

### Frontend Vitest

- `WorkflowForm` renders correct controls for each field_type
- Settings workflow list CRUD operations
