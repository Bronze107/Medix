# ComfyUI Provider Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ComfyUI as a local image generation provider alongside xAI, with named workflow management and `#param` convention for dynamic UI forms.

**Architecture:** New `ComfyuiProvider` implements `ImageProvider` trait. Named workflows stored in SQLite with full ComfyUI JSON. Nodes with `#param` titles generate dynamic form controls on the frontend. Reuses existing ImageQueue, staging, and import lifecycle unchanged.

**Tech Stack:** Rust (reqwest, serde_json, async_trait), TypeScript/React, SQLite

---

### Task 1: Database Migration + Settings + DB CRUD

**Files:**
- Modify: `src-tauri/src/db/mod.rs` — migration 0024
- Create: `src-tauri/src/db/comfyui.rs` — workflow CRUD functions
- Modify: `src-tauri/src/settings/mod.rs` — new setting keys + defaults

- [ ] **Step 1: Add DB migration 0024 for comfyui_workflows table**

In `src-tauri/src/db/mod.rs`, after the 0023 migration block, add:

```rust
// 0024_comfyui_workflows
{
    conn.execute_batch(
        "INSERT OR IGNORE INTO _migrations (name) VALUES ('0024_comfyui_workflows');
         CREATE TABLE IF NOT EXISTS comfyui_workflows (
             id TEXT PRIMARY KEY,
             name TEXT NOT NULL,
             workflow_type TEXT NOT NULL DEFAULT 'generate',
             workflow_json TEXT NOT NULL,
             created_at TEXT NOT NULL DEFAULT (datetime('now')),
             updated_at TEXT NOT NULL DEFAULT (datetime('now'))
         );",
    )?;
}
```

- [ ] **Step 2: Run `cargo check` to verify migration compiles**

Run: `cd src-tauri && cargo check 2>&1`
Expected: Compiles clean

- [ ] **Step 3: Create `src-tauri/src/db/comfyui.rs` with CRUD functions**

```rust
use serde::Serialize;
use tauri::AppHandle;

use crate::db;

#[derive(Debug, Clone, Serialize)]
pub struct ComfyWorkflow {
    pub id: String,
    pub name: String,
    pub workflow_type: String,
    pub workflow_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowParam {
    pub node_id: String,
    pub param_name: String,
    pub widget_name: String,
    pub default_value: String,
    pub field_type: String,
    pub order_index: usize,
}

pub fn comfyui_workflow_list(app: &AppHandle, workflow_type: Option<&str>) -> Result<Vec<ComfyWorkflow>, String> {
    let conn = db::get_conn(app)?;
    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(t) = workflow_type {
        (
            "SELECT id, name, workflow_type, workflow_json, created_at, updated_at FROM comfyui_workflows WHERE workflow_type = ?1 ORDER BY updated_at DESC".into(),
            vec![Box::new(t.to_string())],
        )
    } else {
        (
            "SELECT id, name, workflow_type, workflow_json, created_at, updated_at FROM comfyui_workflows ORDER BY workflow_type, updated_at DESC".into(),
            vec![],
        )
    };
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = conn.prepare(&sql).map_err(|e| e.to_string())?
        .query_map(param_refs.as_slice(), |row| {
            Ok(ComfyWorkflow {
                id: row.get(0)?,
                name: row.get(1)?,
                workflow_type: row.get(2)?,
                workflow_json: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        }).map_err(|e| e.to_string())?;
    let mut list = Vec::new();
    for r in rows { list.push(r.map_err(|e| e.to_string())?); }
    Ok(list)
}

pub fn comfyui_workflow_get(app: &AppHandle, id: &str) -> Result<ComfyWorkflow, String> {
    let conn = db::get_conn(app)?;
    conn.query_row(
        "SELECT id, name, workflow_type, workflow_json, created_at, updated_at FROM comfyui_workflows WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(ComfyWorkflow {
                id: row.get(0)?,
                name: row.get(1)?,
                workflow_type: row.get(2)?,
                workflow_json: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    ).map_err(|e| e.to_string())
}

pub fn comfyui_workflow_create(app: &AppHandle, name: &str, workflow_type: &str, workflow_json: &str) -> Result<ComfyWorkflow, String> {
    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let conn = db::get_conn(app)?;
    conn.execute(
        "INSERT INTO comfyui_workflows (id, name, workflow_type, workflow_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, name, workflow_type, workflow_json, now, now],
    ).map_err(|e| e.to_string())?;
    Ok(ComfyWorkflow { id, name: name.into(), workflow_type: workflow_type.into(), workflow_json: workflow_json.into(), created_at: now.clone(), updated_at: now })
}

pub fn comfyui_workflow_update(app: &AppHandle, id: &str, name: &str, workflow_json: &str) -> Result<ComfyWorkflow, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let conn = db::get_conn(app)?;
    conn.execute(
        "UPDATE comfyui_workflows SET name = ?1, workflow_json = ?2, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![name, workflow_json, now, id],
    ).map_err(|e| e.to_string())?;
    comfyui_workflow_get(app, id)
}

pub fn comfyui_workflow_delete(app: &AppHandle, id: &str) -> Result<(), String> {
    let conn = db::get_conn(app)?;
    conn.execute("DELETE FROM comfyui_workflows WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 4: Register `pub mod comfyui;` in `src-tauri/src/db/mod.rs`**

Insert after the existing mod declarations near the top.

- [ ] **Step 5: Add ComfyUI settings keys and defaults in `src-tauri/src/settings/mod.rs`**

Add constants after existing image API keys (around line 281):

```rust
pub const KEY_COMFYUI_BASE_URL: &str = "comfyui_base_url";
pub const KEY_COMFYUI_TIMEOUT_SECS: &str = "comfyui_timeout_secs";
```

Add getters:

```rust
pub fn get_comfyui_base_url(app: &AppHandle) -> String {
    get(app, KEY_COMFYUI_BASE_URL)
        .unwrap_or_default()
        .or_else(|| {
            if get_image_api_provider(app) == "comfyui" {
                Some("http://127.0.0.1:8188".to_string())
            } else { None }
        })
        .unwrap_or_default()
}

pub fn get_comfyui_timeout_secs(app: &AppHandle) -> u64 {
    get(app, KEY_COMFYUI_TIMEOUT_SECS)
        .and_then(|v| v.parse().ok())
        .unwrap_or(300)
}
```

Update `get_image_api_base_url` in the settings module: Add `"comfyui" => "http://127.0.0.1:8188".to_string()` to the existing match (already present at line 321).

- [ ] **Step 6: `cargo check` compiles**

Run: `cd src-tauri && cargo check 2>&1`
Expected: Compiles clean

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/mod.rs src-tauri/src/db/comfyui.rs src-tauri/src/settings/mod.rs
git commit -m "feat(comfyui): add DB migration, workflow CRUD, and settings keys"
```

---

### Task 2: WorkflowManager — Parameter Parsing and Injection

**Files:**
- Create: `src-tauri/src/ai/imagine/workflow.rs`

- [ ] **Step 1: Write the WorkflowManager unit tests first**

In `src-tauri/src/ai/imagine/workflow.rs`, include a `#[cfg(test)]` module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_workflow_json(nodes: &str) -> String {
        format!(r#"{{"nodes":{},"links":[],"groups":[]}}"#, nodes)
    }

    #[test]
    fn test_parse_params_basic() {
        let json = make_workflow_json(r#"[
            {"id":"6","type":"CLIPTextEncode","title":"#prompt","widgets_values":["hello"]},
            {"id":"7","type":"KSampler","title":"#steps=20:slider","widgets_values":[20,7,1]}
        ]"#);
        let params = WorkflowManager::parse_params(&json).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].param_name, "prompt");
        assert_eq!(params[0].field_type, "multiline");
        assert_eq!(params[0].default_value, "hello");
        assert_eq!(params[1].param_name, "steps");
        assert_eq!(params[1].field_type, "slider");
        assert_eq!(params[1].default_value, "20");
    }

    #[test]
    fn test_parse_rejects_no_hash_nodes() {
        let json = make_workflow_json(r#"[{"id":"1","type":"CheckpointLoaderSimple","title":"Load Checkpoint"}]"#);
        assert!(WorkflowManager::parse_params(&json).is_err());
    }

    #[test]
    fn test_parse_duplicate_param_names() {
        let json = make_workflow_json(r#"[
            {"id":"6","type":"CLIPTextEncode","title":"#prompt"},
            {"id":"8","type":"CLIPTextEncode","title":"#prompt"}
        ]"#);
        assert!(WorkflowManager::parse_params(&json).is_err());
    }

    #[test]
    fn test_inject_params() {
        let json = make_workflow_json(r#"[
            {"id":"6","type":"CLIPTextEncode","title":"#prompt","widgets_values":[""]},
            {"id":"7","type":"KSampler","title":"#steps=20","widgets_values":[20,7,1],"inputs":{"seed":0,"steps":20,"cfg":7,"denoise":1}}
        ]"#);
        let mut values = std::collections::HashMap::new();
        values.insert("prompt".to_string(), "a cat".to_string());
        values.insert("steps".to_string(), "30".to_string());
        let modified = WorkflowManager::inject(&json, &values).unwrap();
        assert!(modified.contains(r#""a cat""#));
        // steps value 30 should appear in widgets_values
        assert!(modified.contains("30"));
    }
}
```

- [ ] **Step 2: Run tests, expect failures**

Run: `cd src-tauri && cargo test --lib workflow 2>&1`
Expected: FAIL — module not found

- [ ] **Step 3: Implement WorkflowManager**

```rust
use std::collections::HashMap;

use serde_json::Value;

pub struct WorkflowManager;

impl WorkflowManager {
    /// Parse workflow JSON, extracting #param metadata from node titles.
    /// Returns ordered Vec<WorkflowParam> or an error if no # nodes found
    /// or duplicate param names exist.
    pub fn parse_params(workflow_json: &str) -> Result<Vec<crate::db::comfyui::WorkflowParam>, String> {
        let root: Value = serde_json::from_str(workflow_json)
            .map_err(|e| format!("Invalid workflow JSON: {}", e))?;
        let nodes = root["nodes"].as_array()
            .ok_or("Workflow JSON missing 'nodes' array")?;

        let mut params = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        for node in nodes {
            let title = node["title"].as_str().unwrap_or("");
            // Also check _meta.title (ComfyUI API format)
            let title = if title.is_empty() {
                node["_meta"]["title"].as_str().unwrap_or("")
            } else { title };

            if !title.starts_with('#') { continue; }

            let raw = &title[1..]; // strip #
            // Parse: param_name[=default][:type]
            let (param_name, default_value, field_type) = Self::parse_title(raw, node);

            if !seen_names.insert(param_name.clone()) {
                return Err(format!("Duplicate param name: #{}", param_name));
            }

            params.push(crate::db::comfyui::WorkflowParam {
                node_id: node["id"].as_str().or_else(|| node["id"].as_number().map(|_| ""))
                    .unwrap_or("").to_string(),
                param_name: param_name.clone(),
                widget_name: Self::detect_widget_name(node, &field_type),
                default_value,
                field_type,
                order_index: params.len(),
            });
        }

        if params.is_empty() {
            return Err("No #param nodes found in workflow JSON".to_string());
        }

        Ok(params)
    }

    fn parse_title(raw: &str, node: &Value) -> (String, String, String) {
        // default_value defaults to empty
        let (base, default_value) = if let Some(eq) = raw.find('=') {
            (raw[..eq].to_string(), raw[eq+1..].to_string())
        } else {
            (raw.to_string(), String::new())
        };

        // Check for :type suffix on base
        let (param_name, field_type) = if let Some(colon) = base.find(':') {
            (base[..colon].to_string(), base[colon+1..].to_string())
        } else {
            // Auto-infer from node type
            let class_type = node["type"].as_str().or_else(|| node["class_type"].as_str()).unwrap_or("");
            let inferred = Self::infer_field_type(class_type);
            (base, inferred)
        };

        (param_name, default_value, field_type)
    }

    fn infer_field_type(class_type: &str) -> String {
        match class_type {
            "CLIPTextEncode" => "multiline".into(),
            "KSampler" | "KSamplerAdvanced" => "slider".into(),
            "LoadImage" => "image_selector".into(),
            _ => "text".into(),
        }
    }

    fn detect_widget_name(node: &Value, field_type: &str) -> String {
        // Try to find the primary widget name from inputs
        if let Some(inputs) = node["inputs"].as_object() {
            match field_type {
                "multiline" | "text" => {
                    if inputs.contains_key("text") { return "text".into(); }
                }
                "seed" | "slider" | "number" => {
                    if inputs.contains_key("seed") { return "seed".into(); }
                    if inputs.contains_key("steps") { return "steps".into(); }
                    if inputs.contains_key("cfg") { return "cfg".into(); }
                    if inputs.contains_key("denoise") { return "denoise".into(); }
                }
                "image_selector" => {
                    if inputs.contains_key("image") { return "image".into(); }
                }
                _ => {}
            }
        }
        // Fallback: use param_name lowercased
        "".into()
    }

    /// Inject form values into workflow JSON nodes with matching #param titles.
    pub fn inject(workflow_json: &str, values: &HashMap<String, String>) -> Result<String, String> {
        let mut root: Value = serde_json::from_str(workflow_json)
            .map_err(|e| format!("Invalid workflow JSON: {}", e))?;

        let nodes = root["nodes"].as_array_mut()
            .ok_or("Workflow JSON missing 'nodes' array")?;

        for node in nodes.iter_mut() {
            let title = node["title"].as_str().unwrap_or("").to_string();
            let title = if title.is_empty() {
                node["_meta"]["title"].as_str().unwrap_or("").to_string()
            } else { title };

            if !title.starts_with('#') { continue; }

            let raw = &title[1..];
            let param_name = raw.split('=').next().unwrap_or(raw)
                .split(':').next().unwrap_or(raw);

            if let Some(value) = values.get(param_name) {
                // Update widgets_values array if present
                if let Some(widgets) = node["widgets_values"].as_array() {
                    // Replace first widget value with the user's input
                    // For CLIPTextEncode, it's just ["text"]
                    // For KSampler, it's [seed, steps, cfg, denoise]
                    let idx = Self::widget_index_for_param(param_name);
                    if let Some(wv) = node["widgets_values"].as_array_mut() {
                        if wv.len() > idx {
                            // Try to parse as number first
                            if let Ok(n) = value.parse::<f64>() {
                                wv[idx] = serde_json::Value::Number(
                                    serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0))
                                );
                            } else {
                                wv[idx] = serde_json::Value::String(value.clone());
                            }
                        }
                    }
                }

                // Also update node.inputs field if present (API format)
                if let Some(inputs) = node["inputs"].as_object_mut() {
                    let input_key = Self::input_key_for_param(param_name);
                    if inputs.contains_key(&input_key) {
                        if let Ok(n) = value.parse::<f64>() {
                            inputs[&input_key] = serde_json::Value::Number(
                                serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0))
                            );
                        } else {
                            inputs[&input_key] = serde_json::Value::String(value.clone());
                        }
                    }
                }
            }
        }

        serde_json::to_string(&root).map_err(|e| e.to_string())
    }

    fn widget_index_for_param(param_name: &str) -> usize {
        match param_name {
            "steps" => 1,
            "cfg" => 2,
            "denoise" => 3,
            _ => 0, // default: first widget (prompt, seed)
        }
    }

    fn input_key_for_param(param_name: &str) -> String {
        match param_name {
            "prompt" | "negative_prompt" => "text".into(),
            _ => param_name.to_string(),
        }
    }
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cd src-tauri && cargo test --lib workflow 2>&1`
Expected: All 4 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/imagine/workflow.rs
git commit -m "feat(comfyui): add WorkflowManager with #param parsing and injection"
```

---

### Task 3: ComfyuiProvider — ImageProvider Implementation

**Files:**
- Create: `src-tauri/src/ai/imagine/comfyui.rs`
- Modify: `src-tauri/src/ai/imagine/mod.rs` — register module + factory update

- [ ] **Step 1: Create `src-tauri/src/ai/imagine/comfyui.rs`**

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;

use super::{EditParams, GenerateParams, GeneratedImage, ImageProvider, ImagineError};
use super::workflow::WorkflowManager;
use crate::db::comfyui::ComfyWorkflow;

pub struct ComfyuiProvider {
    base_url: String,
    timeout_secs: u64,
    workflow: ComfyWorkflow,
    client: reqwest::Client,
}

impl ComfyuiProvider {
    pub fn new(base_url: String, timeout_secs: u64, workflow: ComfyWorkflow) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build ComfyUI HTTP client");
        Self { base_url, timeout_secs, workflow, client }
    }

    async fn submit_and_wait(
        &self,
        values: HashMap<String, String>,
        params: &GenerateParams,
    ) -> Result<Vec<GeneratedImage>, ImagineError> {
        // 1. Inject params into workflow JSON
        let workflow_json = WorkflowManager::inject(&self.workflow.workflow_json, &values)
            .map_err(|e| ImagineError::Api(e))?;

        let workflow_value: Value = serde_json::from_str(&workflow_json)
            .map_err(|e| ImagineError::Api(format!("Invalid injected workflow: {}", e)))?;

        let body = serde_json::json!({
            "prompt": workflow_value,
            "client_id": "medix"
        });

        // 2. POST /prompt
        let submit_url = format!("{}/prompt", self.base_url);
        let resp = self.client.post(&submit_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    ImagineError::Api(format!("ComfyUI 未在 {} 运行", self.base_url))
                } else {
                    ImagineError::Http(e)
                }
            })?;

        let resp_json: Value = resp.json().await.map_err(ImagineError::Http)?;
        let prompt_id = resp_json["prompt_id"].as_str()
            .ok_or(ImagineError::Api("No prompt_id in ComfyUI response".into()))?
            .to_string();

        // 3. Poll GET /history/{prompt_id} until complete
        let start = Instant::now();
        let history_url = format!("{}/history/{}", self.base_url, prompt_id);
        loop {
            if start.elapsed() > Duration::from_secs(self.timeout_secs) {
                return Err(ImagineError::Api(format!("任务超时 ({}s)", self.timeout_secs)));
            }

            let hist_resp = self.client.get(&history_url).send().await.map_err(ImagineError::Http)?;
            let hist_json: Value = hist_resp.json().await.map_err(ImagineError::Http)?;

            // history returns {} while processing, object with outputs when done
            if let Some(outputs) = hist_json[&prompt_id]["outputs"].as_object() {
                // 4. Download output images
                let mut images = Vec::new();
                for (_node_id, node_output) in outputs {
                    if let Some(img_list) = node_output["images"].as_array() {
                        for img_info in img_list {
                            let filename = img_info["filename"].as_str().unwrap_or("");
                            let subfolder = img_info["subfolder"].as_str().unwrap_or("");
                            let img_type = img_info["type"].as_str().unwrap_or("output");

                            let dl_url = format!(
                                "{}/view?filename={}&subfolder={}&type={}",
                                self.base_url, filename, subfolder, img_type
                            );

                            let dl_resp = self.client.get(&dl_url)
                                .send().await.map_err(ImagineError::Http)?;
                            let data = dl_resp.bytes().await.map_err(ImagineError::Http)?;

                            let mime_type = if filename.ends_with(".png") {
                                "image/png"
                            } else {
                                "image/jpeg"
                            };

                            images.push(GeneratedImage {
                                mime_type: mime_type.to_string(),
                                data: data.to_vec(),
                            });
                        }
                    }
                }

                if images.is_empty() {
                    return Err(ImagineError::EmptyResponse);
                }
                return Ok(images);
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

#[async_trait]
impl ImageProvider for ComfyuiProvider {
    async fn generate(&self, params: &GenerateParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        let mut values = HashMap::new();
        // Map GenerateParams to #param values
        values.insert("prompt".to_string(), params.prompt.clone());
        // aspect_ratio and resolution are ignored by default —
        // user can add #aspect_ratio / #resolution nodes to their workflow if needed

        self.submit_and_wait(values, params).await
    }

    async fn edit(&self, params: &EditParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        // 1. Upload source image
        let upload_url = format!("{}/upload/image", self.base_url);

        // Extract base64 from data URL
        let (mime, b64_data) = if let Some(comma) = params.image_data_url.find(',') {
            let data = &params.image_data_url[comma + 1..];
            let mime = if params.image_data_url.contains("image/png") {
                "image/png"
            } else {
                "image/jpeg"
            };
            (mime, data.to_string())
        } else {
            return Err(ImagineError::Api("Invalid image data URL".into()));
        };

        let img_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &b64_data,
        ).map_err(|e| ImagineError::Api(format!("Failed to decode base64: {}", e)))?;

        let part = reqwest::multipart::Part::bytes(img_bytes)
            .file_name(if mime == "image/png" { "input.png".to_string() } else { "input.jpg".to_string() })
            .mime_str(mime).map_err(|e| ImagineError::Api(e.to_string()))?;

        let form = reqwest::multipart::Form::new()
            .part("image", part);

        let upload_resp = self.client.post(&upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(ImagineError::Http)?;

        let upload_json: Value = upload_resp.json().await.map_err(ImagineError::Http)?;
        let uploaded_filename = upload_json["name"].as_str()
            .ok_or(ImagineError::Api("No filename from upload response".into()))?;

        // 2. Build values including input_image
        let mut values = HashMap::new();
        values.insert("prompt".to_string(), params.prompt.clone());
        values.insert("input_image".to_string(), uploaded_filename.to_string());

        self.submit_and_wait(values, &GenerateParams {
            prompt: params.prompt.clone(),
            aspect_ratio: params.aspect_ratio.clone(),
            resolution: params.resolution.clone(),
            n: params.n,
        }).await
    }

    async fn health_check(&self) -> Result<bool, ImagineError> {
        let url = format!("{}/system_stats", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}
```

- [ ] **Step 2: Register module in `src-tauri/src/ai/imagine/mod.rs`**

Add at line 1:

```rust
pub mod comfyui;
pub mod workflow;
```

- [ ] **Step 3: Update factory in `src-tauri/src/ai/imagine/mod.rs`**

Replace `create_provider` signature and body:

```rust
pub fn create_provider(app: &AppHandle, workflow_id: Option<&str>) -> Result<Box<dyn ImageProvider>, String> {
    let provider = settings::get_image_api_provider(app);
    match provider.as_str() {
        "xai" => {
            // ... existing xAI code unchanged ...
        }
        "comfyui" => {
            let wf_id = workflow_id.ok_or("ComfyUI requires a workflow_id")?;
            let workflow = crate::db::comfyui::comfyui_workflow_get(app, wf_id)
                .map_err(|e| format!("Workflow not found: {}", e))?;
            let base_url = settings::get_comfyui_base_url(app);
            let timeout = settings::get_comfyui_timeout_secs(app);
            Ok(Box::new(comfyui::ComfyuiProvider::new(base_url, timeout, workflow)))
        }
        "" => Err("No image API provider configured".to_string()),
        _ => Err(format!("Unknown image provider: {}", provider)),
    }
}
```

Note: xai branch must also handle the `workflow_id: Option<&str>` parameter — just ignore it.

- [ ] **Step 4: `cargo check` compiles**

Run: `cd src-tauri && cargo check 2>&1`
Expected: Compiles clean (may have warnings about unused params in xai branch — ok)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/imagine/comfyui.rs src-tauri/src/ai/imagine/workflow.rs src-tauri/src/ai/imagine/mod.rs
git commit -m "feat(comfyui): add ComfyuiProvider implementing ImageProvider trait"
```

---

### Task 4: Update ImageQueue for workflow_id

**Files:**
- Modify: `src-tauri/src/ai/imagine/queue.rs`

- [ ] **Step 1: Add workflow_id to ImageTask enum and TaskState**

In `src-tauri/src/ai/imagine/queue.rs`, update the `ImageTask` enum to include `workflow_id`:

```rust
pub enum ImageTask {
    Generate {
        task_id: String,
        prompt: String,
        aspect_ratio: String,
        resolution: String,
        n: u32,
        workflow_id: Option<String>,
    },
    Edit {
        task_id: String,
        media_id: String,
        variant_id: Option<String>,
        prompt: String,
        aspect_ratio: String,
        resolution: String,
        n: u32,
        workflow_id: Option<String>,
    },
}
```

- [ ] **Step 2: Add workflow_id to TaskState struct**

```rust
struct TaskState {
    task_id: String,
    task_type: String,
    prompt: String,
    media_id: Option<String>,
    status: String,
    staged: Vec<StagedImage>,
    error: Option<String>,
    created_at: String,
    workflow_id: Option<String>,
}

impl TaskState {
    fn to_info(&self) -> TaskInfo {
        TaskInfo {
            task_id: self.task_id.clone(),
            task_type: self.task_type.clone(),
            prompt: self.prompt.clone(),
            media_id: self.media_id.clone(),
            status: self.status.clone(),
            staged: self.staged.clone(),
            error: self.error.clone(),
            created_at: self.created_at.clone(),
        }
    }
}
```

- [ ] **Step 3: Update process_task to pass workflow_id to create_provider**

In `process_task()`, extract `workflow_id` from the enum variants:

```rust
async fn process_task(
    app: AppHandle,
    tasks: &Arc<Mutex<HashMap<String, TaskState>>>,
    staging_dir: &PathBuf,
    task: ImageTask,
) {
    let (task_id, task_type, prompt, media_id, generate_params, edit_params, workflow_id) = match task {
        ImageTask::Generate { task_id, prompt, aspect_ratio, resolution, n, workflow_id } => {
            // ...
            (task_id, "generate", prompt, None, Some(params), None, workflow_id)
        }
        ImageTask::Edit { task_id, media_id, variant_id, prompt, aspect_ratio, resolution, n, workflow_id } => {
            // ...
            (task_id, "edit", prompt, Some(media_id), None, Some(params), workflow_id)
        }
    };

    // ...

    let provider = match create_provider(&app, workflow_id.as_deref()) {
        // ... rest unchanged
    };
}
```

- [ ] **Step 4: Update command handlers**

Update `image_queue_submit_generate` to accept and pass `workflow_id`:

```rust
#[tauri::command]
pub fn image_queue_submit_generate(
    app: AppHandle,
    prompt: String,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    n: Option<u32>,
    workflow_id: Option<String>,
) -> Result<String, String> {
    // ... insert_task with workflow_id
    let task = ImageTask::Generate {
        task_id: task_id.clone(),
        prompt: prompt.trim().to_string(),
        aspect_ratio: aspect_ratio.unwrap_or_else(|| "auto".to_string()),
        resolution: resolution.unwrap_or_else(|| "1k".to_string()),
        n: n.unwrap_or(1),
        workflow_id: workflow_id.clone(),
    };
    queue.insert_task(TaskState {
        task_id: task_id.clone(),
        task_type: "generate".to_string(),
        prompt: prompt.trim().to_string(),
        media_id: None,
        status: "pending".to_string(),
        staged: Vec::new(),
        error: None,
        created_at: Utc::now().to_rfc3339(),
        workflow_id,
    });
    // ...
}
```

Same update for `image_queue_submit_edit`:

```rust
#[tauri::command]
pub fn image_queue_submit_edit(
    app: AppHandle,
    media_id: String,
    variant_id: Option<String>,
    prompt: String,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    n: Option<u32>,
    workflow_id: Option<String>,
) -> Result<String, String> {
    // ... insert_task with workflow_id and pass to ImageTask::Edit
}
```

- [ ] **Step 5: `cargo check` compiles**

Run: `cd src-tauri && cargo check 2>&1`
Expected: Compiles clean

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ai/imagine/queue.rs
git commit -m "feat(comfyui): add workflow_id to ImageTask queue and submit commands"
```

---

### Task 5: Workflow CRUD Tauri Commands + Main Registration

**Files:**
- Create: `src-tauri/src/commands/comfyui.rs`
- Modify: `src-tauri/src/main.rs` — register commands

- [ ] **Step 1: Create `src-tauri/src/commands/comfyui.rs`**

```rust
use tauri::{command, AppHandle};

use crate::db::comfyui::{self, ComfyWorkflow};
use crate::ai::imagine::workflow::WorkflowManager;

#[command]
pub fn comfyui_workflow_list(
    app: AppHandle,
    workflow_type: Option<String>,
) -> Result<Vec<ComfyWorkflow>, String> {
    comfyui::comfyui_workflow_list(&app, workflow_type.as_deref())
        .map_err(|e| e.to_string())
}

#[command]
pub fn comfyui_workflow_get(
    app: AppHandle,
    id: String,
) -> Result<serde_json::Value, String> {
    let wf = comfyui::comfyui_workflow_get(&app, &id)
        .map_err(|e| e.to_string())?;
    let params = WorkflowManager::parse_params(&wf.workflow_json)
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "id": wf.id,
        "name": wf.name,
        "workflow_type": wf.workflow_type,
        "workflow_json": wf.workflow_json,
        "params": params,
        "created_at": wf.created_at,
        "updated_at": wf.updated_at,
    }))
}

#[command]
pub fn comfyui_workflow_create(
    app: AppHandle,
    name: String,
    workflow_type: String,
    workflow_json: String,
) -> Result<ComfyWorkflow, String> {
    // Validate
    let _ = WorkflowManager::parse_params(&workflow_json)
        .map_err(|e| format!("Invalid workflow: {}", e))?;

    comfyui::comfyui_workflow_create(&app, &name, &workflow_type, &workflow_json)
        .map_err(|e| e.to_string())
}

#[command]
pub fn comfyui_workflow_update(
    app: AppHandle,
    id: String,
    name: String,
    workflow_json: String,
) -> Result<ComfyWorkflow, String> {
    let _ = WorkflowManager::parse_params(&workflow_json)
        .map_err(|e| format!("Invalid workflow: {}", e))?;

    comfyui::comfyui_workflow_update(&app, &id, &name, &workflow_json)
        .map_err(|e| e.to_string())
}

#[command]
pub fn comfyui_workflow_delete(
    app: AppHandle,
    id: String,
) -> Result<(), String> {
    comfyui::comfyui_workflow_delete(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub async fn comfyui_test_connection(
    app: AppHandle,
) -> Result<String, String> {
    use crate::settings;
    use crate::ai::imagine::ImageProvider;

    let wf_id = crate::db::comfyui::comfyui_workflow_list(&app, None)
        .map_err(|e| e.to_string())?
        .first()
        .map(|w| w.id.clone())
        .ok_or("请先保存一个工作流")?;

    let provider = crate::ai::imagine::create_provider(&app, Some(&wf_id))
        .map_err(|e| e.to_string())?;

    match provider.health_check().await {
        Ok(true) => Ok("ComfyUI 连接成功".to_string()),
        Ok(false) => Err("无法连接到 ComfyUI".to_string()),
        Err(e) => Err(e.to_string()),
    }
}
```

- [ ] **Step 2: Register `pub mod comfyui;` in `src-tauri/src/commands/mod.rs`**

Check if there's a `commands/mod.rs` file:

If using `commands/` directory with `mod.rs`, add `pub mod comfyui;` there.
If commands are declared in `main.rs`, add `mod commands;` line accordingly.

- [ ] **Step 3: Register commands in `src-tauri/src/main.rs`**

Add the new imports and register in `invoke_handler`:

At the top (around existing command imports), add:
```rust
use crate::commands::comfyui::{
    comfyui_workflow_list, comfyui_workflow_get, comfyui_workflow_create,
    comfyui_workflow_update, comfyui_workflow_delete, comfyui_test_connection,
};
```

In the `invoke_handler` macro, add the new commands (after the image_queue_* entries):
```rust
comfyui_workflow_list,
comfyui_workflow_get,
comfyui_workflow_create,
comfyui_workflow_update,
comfyui_workflow_delete,
comfyui_test_connection,
```

- [ ] **Step 4: `cargo check` compiles**

Run: `cd src-tauri && cargo check 2>&1`
Expected: Compiles clean

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/comfyui.rs src-tauri/src/main.rs
git commit -m "feat(comfyui): add workflow CRUD Tauri commands"
```

---

### Task 6: Frontend Types + Tauri API Wrappers

**Files:**
- Create: `src/types/comfyui.ts`
- Modify: `src/lib/tauri.ts` — add command wrappers

- [ ] **Step 1: Create `src/types/comfyui.ts`**

```typescript
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

export interface ComfyWorkflowDetail extends ComfyWorkflow {
  params: WorkflowParam[];
}
```

- [ ] **Step 2: Add Tauri API wrappers in `src/lib/tauri.ts`**

Add after existing image queue functions (around line 539):

```typescript
// --- ComfyUI ---

import type { ComfyWorkflow, ComfyWorkflowDetail } from "@/types/comfyui";

export function comfyuiWorkflowList(workflowType?: string): Promise<ComfyWorkflow[]> {
  return invoke("comfyui_workflow_list", { workflowType: workflowType ?? null });
}

export function comfyuiWorkflowGet(id: string): Promise<ComfyWorkflowDetail> {
  return invoke("comfyui_workflow_get", { id });
}

export function comfyuiWorkflowCreate(
  name: string,
  workflowType: string,
  workflowJson: string,
): Promise<ComfyWorkflow> {
  return invoke("comfyui_workflow_create", { name, workflowType, workflowJson });
}

export function comfyuiWorkflowUpdate(
  id: string,
  name: string,
  workflowJson: string,
): Promise<ComfyWorkflow> {
  return invoke("comfyui_workflow_update", { id, name, workflowJson });
}

export function comfyuiWorkflowDelete(id: string): Promise<void> {
  return invoke("comfyui_workflow_delete", { id });
}

export function comfyuiTestConnection(): Promise<string> {
  return invoke("comfyui_test_connection");
}
```

- [ ] **Step 3: Update `imageQueueSubmitGenerate` and `imageQueueSubmitEdit` in `src/lib/tauri.ts`**

Add optional `workflowId` parameter:

```typescript
export function imageQueueSubmitGenerate(
  prompt: string,
  aspectRatio?: string,
  resolution?: string,
  n?: number,
  workflowId?: string | null,
): Promise<string> {
  return invoke("image_queue_submit_generate", {
    prompt, aspectRatio, resolution, n, workflowId: workflowId ?? null,
  });
}

export function imageQueueSubmitEdit(
  mediaId: string,
  variantId: string | null,
  prompt: string,
  aspectRatio?: string,
  resolution?: string,
  n?: number,
  workflowId?: string | null,
): Promise<string> {
  return invoke("image_queue_submit_edit", {
    mediaId, variantId, prompt, aspectRatio, resolution, n, workflowId: workflowId ?? null,
  });
}
```

- [ ] **Step 4: `npx tsc --noEmit` passes**

Run: `npx tsc --noEmit 2>&1`
Expected: No new errors (pre-existing unrelated ones may exist)

- [ ] **Step 5: Commit**

```bash
git add src/types/comfyui.ts src/lib/tauri.ts
git commit -m "feat(comfyui): add frontend types and Tauri command wrappers"
```

---

### Task 7: Settings Page — ComfyUI Section

**Files:**
- Modify: `src/components/Settings/Settings.tsx`

- [ ] **Step 1: Add ComfyUI state variables**

After existing `imageApiProxy` state:

```typescript
// ComfyUI settings
const [comfyuiBaseUrl, setComfyuiBaseUrl] = useState("http://127.0.0.1:8188");
const [comfyuiTimeout, setComfyuiTimeout] = useState(300);
const [comfyuiWorkflows, setComfyuiWorkflows] = useState<ComfyWorkflow[]>([]);
const [comfyuiTesting, setComfyuiTesting] = useState(false);
const [comfyuiTestResult, setComfyuiTestResult] = useState<string | null>(null);
```

Add imports at top:
```typescript
import type { ComfyWorkflow } from "@/types/comfyui";
import {
  comfyuiWorkflowList,
  comfyuiWorkflowCreate,
  comfyuiWorkflowUpdate,
  comfyuiWorkflowDelete,
  comfyuiTestConnection,
} from "@/lib/tauri";
```

- [ ] **Step 2: Add ComfyUI section after the existing "图像生成 API" section (after line 966)**

```tsx
{/* ComfyUI settings — only shown when provider is comfyui */}
{imageApiProvider === "comfyui" && (
  <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4 mt-3">
    <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
      ComfyUI 配置
    </h2>
    <div className="space-y-3">
      <div>
        <label className="mb-1 block text-xs text-[var(--color-text-muted)]">服务地址</label>
        <input
          type="text"
          value={comfyuiBaseUrl}
          onChange={(e) => setComfyuiBaseUrl(e.target.value)}
          onBlur={() => settingsSet("comfyui_base_url", comfyuiBaseUrl)}
          placeholder="http://127.0.0.1:8188"
          className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
        />
      </div>
      <div>
        <label className="mb-1 block text-xs text-[var(--color-text-muted)]">超时(秒)</label>
        <input
          type="number"
          value={comfyuiTimeout}
          onChange={(e) => setComfyuiTimeout(Number(e.target.value))}
          onBlur={() => settingsSet("comfyui_timeout_secs", String(comfyuiTimeout))}
          min={30}
          max={3600}
          className="w-24 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
        />
      </div>
      <button
        onClick={async () => {
          setComfyuiTesting(true);
          setComfyuiTestResult(null);
          try {
            const result = await comfyuiTestConnection();
            setComfyuiTestResult(result);
          } catch (e: any) {
            setComfyuiTestResult(String(e));
          } finally {
            setComfyuiTesting(false);
          }
        }}
        disabled={comfyuiTesting}
        className="rounded border border-[var(--color-border-light)] px-3 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] disabled:opacity-50 active:scale-[0.97] transition-colors"
      >
        {comfyuiTesting ? "测试中..." : "测试连接"}
      </button>
      {comfyuiTestResult && (
        <p className={`text-xs ${comfyuiTestResult.includes("成功") ? "text-[var(--color-success)]" : "text-[var(--color-danger)]"}`}>
          {comfyuiTestResult}
        </p>
      )}
    </div>

    {/* Workflow list */}
    <div className="mt-4">
      <h3 className="mb-2 text-xs font-semibold text-[var(--color-text-secondary)]">工作流</h3>
      {["generate", "edit"].map((wfType) => (
        <div key={wfType} className="mb-3">
          <p className="mb-1 text-[11px] text-[var(--color-text-muted)]">
            {wfType === "generate" ? "文生图" : "图生图"}
          </p>
          {comfyuiWorkflows.filter(w => w.workflow_type === wfType).map((wf) => (
            <div key={wf.id} className="flex items-center justify-between py-1">
              <span className="text-xs text-[var(--color-text-primary)]">{wf.name}</span>
              <div className="flex gap-1">
                <button
                  onClick={() => {
                    const name = window.prompt("工作流名称", wf.name);
                    if (name) {
                      comfyuiWorkflowUpdate(wf.id, name, wf.workflow_json).then(loadWorkflows);
                    }
                  }}
                  className="rounded px-2 py-0.5 text-[11px] text-[var(--color-text-muted)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                >
                  编辑
                </button>
                <button
                  onClick={() => comfyuiWorkflowDelete(wf.id).then(loadWorkflows)}
                  className="rounded px-2 py-0.5 text-[11px] text-[var(--color-text-muted)] hover:text-[var(--color-danger)] hover:bg-[var(--color-danger-soft)] transition-colors"
                >
                  删除
                </button>
              </div>
            </div>
          ))}
          <button
            onClick={() => {
              const name = window.prompt("工作流名称");
              const json = window.prompt("粘贴 ComfyUI workflow JSON (API format)");
              if (name && json) {
                comfyuiWorkflowCreate(name, wfType, json)
                  .then(loadWorkflows)
                  .catch((e) => alert("保存失败: " + String(e)));
              }
            }}
            className="mt-1 text-[11px] text-[var(--color-accent)] hover:text-[var(--color-accent-hover)] transition-colors"
          >
            + 添加{wfType === "generate" ? "文生图" : "图生图"}工作流
          </button>
        </div>
      ))}
    </div>
  </section>
)}
```

- [ ] **Step 3: Add `loadWorkflows` helper and call it on provider change**

```typescript
const loadWorkflows = useCallback(async () => {
  try {
    setComfyuiWorkflows(await comfyuiWorkflowList());
  } catch (e) {
    console.error("Failed to load workflows:", e);
  }
}, []);
```

Call `loadWorkflows()` when `imageApiProvider` changes to `"comfyui"` (add to an existing useEffect or create one). Also load `comfyui_base_url` and `comfyui_timeout_secs` settings in the existing `loadSettingsOnce` callback.

Add to loadSettingsOnce:
```typescript
if (settings.comfyui_base_url) setComfyuiBaseUrl(settings.comfyui_base_url);
if (settings.comfyui_timeout_secs) setComfyuiTimeout(Number(settings.comfyui_timeout_secs));
```

- [ ] **Step 4: Add "comfyui" option to the provider select**

In the select at line 928-933:
```tsx
<option value="comfyui">ComfyUI (本地)</option>
```

- [ ] **Step 5: `npx tsc --noEmit` verifies**

Run: `npx tsc --noEmit 2>&1`
Expected: No new errors

- [ ] **Step 6: Commit**

```bash
git add src/components/Settings/Settings.tsx
git commit -m "feat(comfyui): add ComfyUI settings section with workflow list"
```

---

### Task 8: AiGenPage — Dynamic Workflow Form

**Files:**
- Create: `src/components/AiGenPage/WorkflowForm.tsx`
- Modify: `src/components/AiGenPage/AiGenPage.tsx`

- [ ] **Step 1: Create `src/components/AiGenPage/WorkflowForm.tsx`**

```tsx
import type { ComfyWorkflow, WorkflowParam } from "@/types/comfyui";
import { comfyuiWorkflowList, comfyuiWorkflowGet } from "@/lib/tauri";
import { useEffect, useState } from "react";

interface WorkflowFormProps {
  onParamChange: (values: Record<string, string>) => void;
  onWorkflowChange: (workflowId: string) => void;
}

export function WorkflowForm({ onParamChange, onWorkflowChange }: WorkflowFormProps) {
  const [workflows, setWorkflows] = useState<ComfyWorkflow[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [params, setParams] = useState<WorkflowParam[]>([]);
  const [values, setValues] = useState<Record<string, string>>({});

  useEffect(() => {
    comfyuiWorkflowList("generate").then(setWorkflows).catch(console.error);
  }, []);

  useEffect(() => {
    if (!selectedId) return;
    comfyuiWorkflowGet(selectedId).then((detail) => {
      setParams(detail.params);
      const init: Record<string, string> = {};
      for (const p of detail.params) {
        init[p.param_name] = p.default_value;
      }
      setValues(init);
      onParamChange(init);
      onWorkflowChange(selectedId);
    }).catch(console.error);
  }, [selectedId]);

  useEffect(() => {
    if (!selectedId && workflows.length > 0) {
      setSelectedId(workflows[0].id);
    }
  }, [workflows]);

  useEffect(() => {
    onParamChange(values);
  }, [values]);

  const update = (name: string, value: string) => {
    setValues((prev) => ({ ...prev, [name]: value }));
  };

  return (
    <div className="flex flex-col gap-3">
      <div>
        <label className="mb-1 block text-xs text-[var(--color-text-muted)]">工作流</label>
        <select
          value={selectedId}
          onChange={(e) => setSelectedId(e.target.value)}
          className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
        >
          {workflows.map((w) => (
            <option key={w.id} value={w.id}>{w.name}</option>
          ))}
        </select>
      </div>

      {params.map((p) => (
        <div key={p.node_id + p.param_name}>
          <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
            #{p.param_name}
          </label>
          {renderField(p, values[p.param_name] ?? p.default_value, update)}
        </div>
      ))}
    </div>
  );
}

function renderField(
  p: WorkflowParam,
  value: string,
  onChange: (name: string, value: string) => void,
) {
  switch (p.field_type) {
    case "multiline":
      return (
        <textarea
          value={value}
          onChange={(e) => onChange(p.param_name, e.target.value)}
          rows={4}
          className="w-full resize-none rounded border border-[var(--color-border-light)] bg-[var(--color-bg-secondary)] px-3 py-2 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)]"
        />
      );
    case "seed":
      return (
        <div className="flex gap-2">
          <input
            type="number"
            value={value}
            onChange={(e) => onChange(p.param_name, e.target.value)}
            className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
          />
          <button
            onClick={() => onChange(p.param_name, "-1")}
            className="shrink-0 rounded border border-[var(--color-border-light)] px-2 py-1 text-[11px] text-[var(--color-text-muted)] hover:bg-[var(--color-bg-hover)] active:scale-[0.97]"
          >
            🎲
          </button>
        </div>
      );
    case "slider":
      return (
        <div className="flex items-center gap-2">
          <input
            type="range"
            min={1}
            max={p.param_name === "steps" ? 100 : p.param_name === "cfg" ? 30 : 100}
            step={p.param_name === "cfg" ? 0.5 : 1}
            value={parseFloat(value) || 1}
            onChange={(e) => onChange(p.param_name, e.target.value)}
            className="flex-1"
          />
          <span className="w-10 text-right text-xs text-[var(--color-text-secondary)]">{value}</span>
        </div>
      );
    case "image_selector":
      return (
        <div className="rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-muted)]">
          当前选中的图片（图生图模式自动绑定）
        </div>
      );
    default:
      return (
        <input
          type={p.field_type === "number" ? "number" : "text"}
          value={value}
          onChange={(e) => onChange(p.param_name, e.target.value)}
          className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-xs text-[var(--color-text-primary)] outline-none"
        />
      );
  }
}
```

- [ ] **Step 2: Modify `AiGenPage.tsx` to use provider-specific forms**

Replace the existing fixed parameter panel (aspect ratio, resolution) when provider is "comfyui".

First, add state for provider and workflow:
```typescript
const [provider, setProvider] = useState<string>("");
// Load provider on mount:
useEffect(() => { settingsGet("image_api_provider").then((v) => setProvider(v || "")).catch(() => {}); }, []);
```

Then conditionally render either the existing form or the WorkflowForm:

```tsx
{provider === "comfyui" ? (
  <WorkflowForm
    onParamChange={setWorkflowParams}
    onWorkflowChange={setWorkflowId}
  />
) : (
  // existing aspect ratio + resolution + n controls (unchanged)
)}
```

Add new state variables:
```typescript
const [workflowParams, setWorkflowParams] = useState<Record<string, string>>({});
const [workflowId, setWorkflowId] = useState<string>("");
```

Update `handleSubmit` to pass `workflowId` when provider is "comfyui":
```typescript
const handleSubmit = async () => {
  if (!prompt.trim()) return;
  // For ComfyUI: prompt comes from workflowParams, not the separate input
  const finalPrompt = provider === "comfyui"
    ? (workflowParams.prompt || prompt.trim())
    : prompt.trim();

  setSubmitting(true);
  try {
    await imageQueueSubmitGenerate(
      finalPrompt,
      aspectRatio,
      resolution,
      n,
      provider === "comfyui" ? workflowId : null,
    );
    record(prompt, aspectRatio, resolution);
    setPrompt("");
    await loadTasks();
  } catch (e) {
    console.error("Failed to submit:", e);
  } finally {
    setSubmitting(false);
  }
};
```

In the WorkflowForm, when provider is "comfyui", any prompt-like param value (param_name === "prompt") can be linked directly to the main prompt textarea for history recording.

- [ ] **Step 3: `npx tsc --noEmit` passes**

Run: `npx tsc --noEmit 2>&1`
Expected: No new errors

- [ ] **Step 4: Commit**

```bash
git add src/components/AiGenPage/WorkflowForm.tsx src/components/AiGenPage/AiGenPage.tsx
git commit -m "feat(comfyui): add dynamic workflow form to AiGenPage"
```

---

### Task 9: ImagineDialog — ComfyUI Edit Support

**Files:**
- Modify: `src/components/ImagineDialog/ImagineDialog.tsx`

- [ ] **Step 1: Add workflow support to ImagineDialog**

Read current `src/components/ImagineDialog/ImagineDialog.tsx` to understand structure.

This is a lighter change — just add `workflow_id` pass-through. The interface changes:

```typescript
// When submitting edit:
await imageQueueSubmitEdit(
  mediaId,
  variantId ?? null,
  prompt.trim(),
  undefined, // aspectRatio (not used for ComfyUI)
  undefined, // resolution (not used for ComfyUI)
  undefined, // n
  provider === "comfyui" ? workflowId : null,
);
```

Add a small workflow selector (same component from WorkflowForm or a minimal inline version) filtered to `workflow_type === "edit"`. This is optional — if no edit workflow is configured, show a message "请在设置中配置图生图工作流".

- [ ] **Step 2: `npx tsc --noEmit` passes**

Run: `npx tsc --noEmit 2>&1`
Expected: No new errors

- [ ] **Step 3: Commit**

```bash
git add src/components/ImagineDialog/ImagineDialog.tsx
git commit -m "feat(comfyui): add ComfyUI edit workflow support to ImagineDialog"
```

---

### Task 10: CLI Regression Test + Rust Unit Tests

**Files:**
- Create: `tests/comfyui.sh`

- [ ] **Step 1: Create `tests/comfyui.sh`**

```bash
#!/bin/bash
set -euo pipefail
source "$(dirname "$0")/_helpers.sh"

echo "=== ComfyUI Workflow CRUD Tests ==="

setup_isolated_db "comfyui-test"

# Create workflows via direct SQL (bypasses validation for testability)
exec_sql "INSERT INTO comfyui_workflows (id, name, workflow_type, workflow_json, created_at, updated_at)
  VALUES ('01TEST', 'Test Generate', 'generate', '{\"nodes\":[{\"id\":\"1\",\"type\":\"CLIPTextEncode\",\"title\":\"#prompt\",\"widgets_values\":[\"\"]}]}', datetime('now'), datetime('now'))"

exec_sql "INSERT INTO comfyui_workflows (id, name, workflow_type, workflow_json, created_at, updated_at)
  VALUES ('02TEST', 'Test Edit', 'edit', '{\"nodes\":[{\"id\":\"1\",\"type\":\"LoadImage\",\"title\":\"#input_image\",\"widgets_values\":[\"\"]},{\"id\":\"2\",\"type\":\"CLIPTextEncode\",\"title\":\"#prompt\",\"widgets_values\":[\"\"]}]}', datetime('now'), datetime('now'))"

# List all
count=$(q "SELECT COUNT(*) FROM comfyui_workflows")
check "$count" "2"

# Filter by type
gen_count=$(q "SELECT COUNT(*) FROM comfyui_workflows WHERE workflow_type='generate'")
check "$gen_count" "1"

edit_count=$(q "SELECT COUNT(*) FROM comfyui_workflows WHERE workflow_type='edit'")
check "$edit_count" "1"

# Update
exec_sql "UPDATE comfyui_workflows SET name='Updated Name' WHERE id='01TEST'"
updated=$(q "SELECT name FROM comfyui_workflows WHERE id='01TEST'")
check "$updated" "Updated Name"

# Delete
exec_sql "DELETE FROM comfyui_workflows WHERE id='02TEST'"
remaining=$(q "SELECT COUNT(*) FROM comfyui_workflows")
check "$remaining" "1"

# Settings
exec_sql "INSERT INTO settings (key, value) VALUES ('comfyui_base_url', 'http://localhost:8188')"
base_url=$(q "SELECT value FROM settings WHERE key='comfyui_base_url'")
check "$base_url" "http://localhost:8188"

final_report
```

- [ ] **Step 2: Run CLI tests**

Run: `bash tests/comfyui.sh`
Expected: All checks pass

- [ ] **Step 3: Run full Rust test suite**

Run: `cd src-tauri && cargo test --lib 2>&1`
Expected: All tests pass (including new workflow tests from Task 2)

- [ ] **Step 4: Run full regression suite**

Run: `cd src-tauri && bash ../tests/comfyui.sh && bash ../tests/search.sh && bash ../tests/integrity.sh && bash ../tests/operations.sh && bash ../tests/tags-collections.sh && bash ../tests/cascade.sh && bash ../tests/variants-browse.sh 2>&1`
Expected: All scripts pass

- [ ] **Step 5: Commit**

```bash
git add tests/comfyui.sh
git commit -m "test(comfyui): add CLI regression test for workflow CRUD"
```

---

### Task 11: Rust Unit Tests — Provider + Injection Edge Cases

**Files:**
- Modify: `src-tauri/src/ai/imagine/comfyui.rs` — add test module

- [ ] **Step 1: Add edge case tests to WorkflowManager tests**

In `src-tauri/src/ai/imagine/workflow.rs`, add tests:

```rust
#[test]
fn test_parse_seed_with_default() {
    let json = make_test_json(r#"[
        {"id":"3","type":"KSampler","title":"#seed=-1:seed","widgets_values":[42,20,7,1]}
    ]"#);
    let params = WorkflowManager::parse_params(&json).unwrap();
    assert_eq!(params[0].field_type, "seed");
    assert_eq!(params[0].default_value, "-1");
}

#[test]
fn test_parse_invalid_json() {
    assert!(WorkflowManager::parse_params("not json").is_err());
}

#[test]
fn test_parse_missing_nodes() {
    assert!(WorkflowManager::parse_params(r#"{"stuff":[]}"#).is_err());
}

#[test]
fn test_inject_preserves_other_nodes() {
    let json = make_test_json(r#"[
        {"id":"1","type":"CheckpointLoader","title":"Load","widgets_values":["sd_xl.safetensors"]},
        {"id":"2","type":"CLIPTextEncode","title":"#prompt","widgets_values":[""]}
    ]"#);
    let mut values = HashMap::new();
    values.insert("prompt".into(), "test prompt".into());
    let result = WorkflowManager::inject(&json, &values).unwrap();
    assert!(result.contains("sd_xl.safetensors")); // unchanged node preserved
    assert!(result.contains("test prompt"));
}
```

- [ ] **Step 2: Run all workflow tests**

Run: `cd src-tauri && cargo test --lib workflow 2>&1`
Expected: All 7 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ai/imagine/workflow.rs
git commit -m "test(comfyui): add edge case tests for WorkflowManager"
```
