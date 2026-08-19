use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use super::queue::ImageQueue;
use super::workflow::WorkflowManager;
use super::{EditParams, GenerateParams, GeneratedImage, ImageProvider, ImagineError};
use crate::db::comfyui::ComfyWorkflow;

type ComfyWs = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

pub struct ComfyuiProvider {
    base_url: String,
    timeout_secs: u64,
    workflow: ComfyWorkflow,
    client: reqwest::Client,
    app: AppHandle,
    task_id: String,
}

impl ComfyuiProvider {
    pub fn new(
        base_url: String,
        timeout_secs: u64,
        workflow: ComfyWorkflow,
        app: AppHandle,
        task_id: String,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build ComfyUI HTTP client");
        Self {
            base_url,
            timeout_secs,
            workflow,
            client,
            app,
            task_id,
        }
    }

    /// 拉取 /object_info（失败返回空对象，调用方容错处理）。
    async fn fetch_object_info(&self) -> Value {
        let url = format!("{}/object_info", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => resp.json::<Value>().await.unwrap_or(Value::Null),
            Err(e) => {
                eprintln!("[comfyui] fetch object_info FAILED: {}", e);
                Value::Null
            }
        }
    }

    async fn submit_and_wait(
        &self,
        values: HashMap<String, String>,
        image_data_urls: Vec<String>,
    ) -> Result<Vec<GeneratedImage>, ImagineError> {
        let params = WorkflowManager::parse_params(&self.workflow.workflow_json)
            .map_err(ImagineError::Api)?;
        let object_info = self.fetch_object_info().await;

        let mut api_prompt =
            WorkflowManager::standard_to_api(&self.workflow.workflow_json, &object_info)
                .map_err(ImagineError::Api)?;

        // 编辑模式：上传所有源图，按序绑定到 image_selector 参数（LoadImage image widget）。
        if !image_data_urls.is_empty() {
            let mut filenames = Vec::new();
            for url in &image_data_urls {
                filenames.push(self.upload_image(url).await?);
            }
            let mut idx = 0usize;
            for p in &params {
                if p.field_type == "image_selector" && idx < filenames.len() {
                    if let Some(node) = api_prompt.get_mut(&p.node_id) {
                        if let Some(inputs) =
                            node.get_mut("inputs").and_then(|i| i.as_object_mut())
                        {
                            inputs.insert(
                                p.widget_name.clone(),
                                Value::String(filenames[idx].clone()),
                            );
                        }
                    }
                    idx += 1;
                }
            }
        }

        WorkflowManager::inject(&mut api_prompt, &values, &params);

        // 先建立 WebSocket，再提交，避免快速任务在 WS 连接前就完成而错过执行事件。
        let ws = self.connect_ws().await;
        let prompt_id = self.submit(&api_prompt).await?;

        // 登记 prompt_id 供 image_queue_cancel 定向中断。
        if let Some(q) = self.app.try_state::<ImageQueue>() {
            q.set_prompt_id(&self.task_id, prompt_id.clone());
        }

        match ws {
            Ok(mut stream) => match self.wait_ws(&mut stream, &prompt_id).await {
                Ok(()) => {}
                Err(ImagineError::WebSocket(msg)) => {
                    eprintln!("[comfyui] ws lost ({}), falling back to polling", msg);
                    self.wait_poll(&prompt_id).await?;
                }
                Err(e) => return Err(e),
            },
            Err(msg) => {
                eprintln!("[comfyui] ws connect failed ({}), polling", msg);
                self.wait_poll(&prompt_id).await?;
            }
        }

        self.collect_images(&prompt_id).await
    }

    async fn connect_ws(&self) -> Result<ComfyWs, ImagineError> {
        let ws_url = Self::ws_url(&self.base_url, &self.task_id);
        tokio_tungstenite::connect_async(&ws_url)
            .await
            .map(|(ws, _)| ws)
            .map_err(|e| ImagineError::WebSocket(e.to_string()))
    }

    async fn submit(&self, api_prompt: &Value) -> Result<String, ImagineError> {
        let body = serde_json::json!({
            "prompt": api_prompt,
            "client_id": self.task_id,
        });
        let url = format!("{}/prompt", self.base_url);
        let resp = self.client.post(&url).json(&body).send().await.map_err(|e| {
            if e.is_connect() {
                ImagineError::Api(format!("ComfyUI 未运行于 {}", self.base_url))
            } else {
                ImagineError::Http(e)
            }
        })?;
        let status = resp.status();
        let text = resp.text().await.map_err(ImagineError::Http)?;
        let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);

        if !status.is_success() || json["error"].is_object() || json["error"].is_string() {
            return Err(ImagineError::Api(Self::format_prompt_error(&json, status.as_u16())));
        }

        json["prompt_id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| {
                ImagineError::Api(format!(
                    "ComfyUI 响应缺少 prompt_id：{}",
                    text.chars().take(200).collect::<String>()
                ))
            })
    }

    /// 修复：POST /prompt 失败时 `error` 是对象，需解析 type/message/details + node_errors。
    fn format_prompt_error(json: &Value, status: u16) -> String {
        let mut msg = format!("ComfyUI 校验失败 (HTTP {})：", status);
        if let Some(err) = json["error"].as_object() {
            let t = err["type"].as_str().unwrap_or("");
            let m = err["message"].as_str().unwrap_or("");
            let d = err["details"].as_str().unwrap_or("");
            msg.push_str(&format!("{}: {} {}", t, m, d));
        } else if let Some(s) = json["error"].as_str() {
            msg.push_str(s);
        } else {
            msg.push_str("未知错误");
        }
        if let Some(ne) = json["node_errors"].as_object() {
            if !ne.is_empty() {
                msg.push_str(" | 节点错误: ");
                let parts: Vec<String> = ne
                    .iter()
                    .map(|(nid, info)| {
                        let errs = info["errors"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|e| e["message"].as_str().map(String::from))
                                    .collect::<Vec<String>>()
                            })
                            .unwrap_or_default();
                        format!("节点 {}: {}", nid, errs.join("; "))
                    })
                    .collect();
                msg.push_str(&parts.join(", "));
            }
        }
        msg
    }

    /// 通过 WebSocket 等待执行结果，实时透出进度。
    async fn wait_ws(&self, ws: &mut ComfyWs, prompt_id: &str) -> Result<(), ImagineError> {
        let deadline = Instant::now() + Duration::from_secs(self.timeout_secs);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let _ = ws.close(None).await;
                return Err(ImagineError::Api(format!(
                    "任务在 {}s 内未完成",
                    self.timeout_secs
                )));
            }
            let next = tokio::time::timeout(remaining, ws.next())
                .await
                .map_err(|_| {
                    ImagineError::Api(format!("任务在 {}s 内未完成", self.timeout_secs))
                })?;
            let Some(frame) = next else { break };
            let frame = frame.map_err(|e| ImagineError::WebSocket(e.to_string()))?;

            match frame {
                tokio_tungstenite::tungstenite::protocol::Message::Text(text) => {
                    let raw = String::from(text);
                    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
                    let ty = v["type"].as_str().unwrap_or("");
                    let data = &v["data"];
                    match ty {
                        "progress" => {
                            if data["prompt_id"].as_str() == Some(prompt_id) {
                                let value = data["value"].as_u64().unwrap_or(0) as u32;
                                let max = data["max"].as_u64().unwrap_or(0) as u32;
                                self.emit_progress(value, max);
                            }
                        }
                        "execution_success" => {
                            if data["prompt_id"].as_str() == Some(prompt_id) {
                                let _ = ws.close(None).await;
                                return Ok(());
                            }
                        }
                        "execution_error" => {
                            if data["prompt_id"].as_str() == Some(prompt_id) {
                                let msg = data["exception_message"]
                                    .as_str()
                                    .unwrap_or("未知错误")
                                    .to_string();
                                let _ = ws.close(None).await;
                                return Err(ImagineError::Api(format!(
                                    "ComfyUI 执行失败：{}",
                                    msg
                                )));
                            }
                        }
                        "execution_interrupted" => {
                            if data["prompt_id"].as_str() == Some(prompt_id) {
                                let _ = ws.close(None).await;
                                return Err(ImagineError::Api("任务已中断".into()));
                            }
                        }
                        _ => {}
                    }
                }
                tokio_tungstenite::tungstenite::protocol::Message::Close(_) => break,
                _ => {} // binary/ping/pong 忽略
            }
        }
        Err(ImagineError::WebSocket("连接提前关闭".into()))
    }

    /// WS 连接失败时的兜底：轮询 history。
    async fn wait_poll(&self, prompt_id: &str) -> Result<(), ImagineError> {
        let start = Instant::now();
        let history_url = format!("{}/history/{}", self.base_url, prompt_id);
        loop {
            if start.elapsed() > Duration::from_secs(self.timeout_secs) {
                return Err(ImagineError::Api(format!(
                    "任务在 {}s 内未完成",
                    self.timeout_secs
                )));
            }
            let resp = self
                .client
                .get(&history_url)
                .send()
                .await
                .map_err(ImagineError::Http)?;
            let hist: Value = resp.json().await.map_err(ImagineError::Http)?;
            if let Some(entry) = hist.get(prompt_id) {
                if let Some(status) = entry["status"].as_object() {
                    if status.get("completed").and_then(|v| v.as_bool()) == Some(true) {
                        let status_str = status["status_str"].as_str().unwrap_or("");
                        if status_str == "error" {
                            let msg = Self::extract_history_error(entry);
                            return Err(ImagineError::Api(format!(
                                "ComfyUI 执行失败：{}",
                                msg
                            )));
                        }
                        return Ok(());
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// 修复：history.status.messages 是 ["execution_error", {...}] 元组数组，
    /// 需从元组第二个元素取 exception_message。
    fn extract_history_error(entry: &Value) -> String {
        let mut out = String::new();
        if let Some(messages) = entry["status"]["messages"].as_array() {
            for m in messages {
                if let Some(arr) = m.as_array() {
                    if arr.first().and_then(|v| v.as_str()) == Some("execution_error") {
                        if let Some(data) = arr.get(1) {
                            if let Some(emsg) = data["exception_message"].as_str() {
                                if !out.is_empty() {
                                    out.push_str("; ");
                                }
                                out.push_str(emsg);
                            }
                        }
                    }
                }
            }
        }
        if out.is_empty() {
            if let Some(s) = entry["status"]["status_str"].as_str() {
                out.push_str(s);
            }
        }
        out
    }

    async fn collect_images(&self, prompt_id: &str) -> Result<Vec<GeneratedImage>, ImagineError> {
        let history_url = format!("{}/history/{}", self.base_url, prompt_id);
        // ComfyUI 先发 execution_success 事件、稍后才把条目写入 history（task_done 落库），
        // 因此成功信号后立即查询可能查不到，需要短暂重试等待落库。
        let start = Instant::now();
        let entry = loop {
            let resp = self
                .client
                .get(&history_url)
                .send()
                .await
                .map_err(ImagineError::Http)?;
            let hist: Value = resp.json().await.map_err(ImagineError::Http)?;
            if let Some(entry) = hist.get(prompt_id) {
                break entry.clone();
            }
            if start.elapsed() > Duration::from_secs(10) {
                return Err(ImagineError::Api("history 中无此任务".into()));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        };
        let outputs = entry["outputs"]
            .as_object()
            .ok_or(ImagineError::EmptyResponse)?;

        let result_nodes = WorkflowManager::result_node_ids(&self.workflow.workflow_json);
        let mut images = Vec::new();
        let mut collected = false;

        // 优先收集 linearData.outputs 声明的结果节点。
        for rn in &result_nodes {
            if let Some(no) = outputs.get(rn) {
                if let Some(list) = no["images"].as_array() {
                    for info in list {
                        images.push(self.download(info).await?);
                    }
                    if !images.is_empty() {
                        collected = true;
                    }
                }
            }
        }
        // 兜底：扫描所有输出节点的 images。
        if !collected {
            for no in outputs.values() {
                if let Some(list) = no["images"].as_array() {
                    for info in list {
                        images.push(self.download(info).await?);
                    }
                }
            }
        }
        if images.is_empty() {
            return Err(ImagineError::EmptyResponse);
        }
        Ok(images)
    }

    async fn download(&self, info: &Value) -> Result<GeneratedImage, ImagineError> {
        let filename = info["filename"].as_str().unwrap_or("");
        let subfolder = info["subfolder"].as_str().unwrap_or("");
        let ty = info["type"].as_str().unwrap_or("output");
        let url = format!(
            "{}/view?filename={}&subfolder={}&type={}",
            self.base_url, filename, subfolder, ty
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(ImagineError::Http)?;
        let data = resp.bytes().await.map_err(ImagineError::Http)?;
        Ok(GeneratedImage {
            mime_type: Self::guess_mime(&data, filename),
            data: data.to_vec(),
        })
    }

    fn guess_mime(data: &[u8], filename: &str) -> String {
        if let Ok(fmt) = image::guess_format(data) {
            return match fmt {
                image::ImageFormat::Png => "image/png",
                image::ImageFormat::Jpeg => "image/jpeg",
                image::ImageFormat::WebP => "image/webp",
                image::ImageFormat::Gif => "image/gif",
                image::ImageFormat::Bmp => "image/bmp",
                _ => {
                    if filename.ends_with(".png") {
                        "image/png"
                    } else {
                        "image/jpeg"
                    }
                }
            }
            .to_string();
        }
        if filename.ends_with(".png") {
            "image/png".to_string()
        } else {
            "image/jpeg".to_string()
        }
    }

    async fn upload_image(&self, image_data_url: &str) -> Result<String, ImagineError> {
        let (mime, b64_data) = if let Some(comma) = image_data_url.find(',') {
            let data = &image_data_url[comma + 1..];
            let mime = if image_data_url.contains("image/png") {
                "image/png"
            } else {
                "image/jpeg"
            };
            (mime, data.to_string())
        } else {
            return Err(ImagineError::Api("无效的图片 data URL".into()));
        };

        let img_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &b64_data,
        )
        .map_err(|e| ImagineError::Api(format!("base64 解码失败：{}", e)))?;

        let part = reqwest::multipart::Part::bytes(img_bytes)
            .file_name(if mime == "image/png" {
                "input.png"
            } else {
                "input.jpg"
            })
            .mime_str(mime)
            .map_err(|e| ImagineError::Api(e.to_string()))?;
        let form = reqwest::multipart::Form::new().part("image", part);

        let url = format!("{}/upload/image", self.base_url);
        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(ImagineError::Http)?;
        let text = resp.text().await.map_err(ImagineError::Http)?;
        let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        json["name"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| ImagineError::Api("上传图片失败，响应无文件名".into()))
    }

    fn emit_progress(&self, value: u32, max: u32) {
        if let Some(q) = self.app.try_state::<ImageQueue>() {
            q.set_progress(&self.task_id, value, max);
        }
        let _ = self.app.emit(
            "image-queue-progress",
            serde_json::json!({ "task_id": self.task_id, "value": value, "max": max }),
        );
    }

    fn ws_url(base_url: &str, client_id: &str) -> String {
        let scheme = if base_url.starts_with("https") { "wss" } else { "ws" };
        let rest = base_url
            .replacen("https://", "", 1)
            .replacen("http://", "", 1);
        format!("{}://{}/ws?clientId={}", scheme, rest, client_id)
    }
}

#[async_trait]
impl ImageProvider for ComfyuiProvider {
    async fn generate(&self, params: &GenerateParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        eprintln!(
            "[comfyui] generate: workflow={} values={:?}",
            self.workflow.name, params.workflow_values
        );
        let result = self
            .submit_and_wait(params.workflow_values.clone(), Vec::new())
            .await;
        if let Err(e) = &result {
            eprintln!("[comfyui] generate FAILED: {}", e);
        }
        result
    }

    async fn edit(&self, params: &EditParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        eprintln!(
            "[comfyui] edit: workflow={} values={:?} images={}",
            self.workflow.name,
            params.workflow_values,
            params.image_data_urls.len()
        );
        let result = self
            .submit_and_wait(params.workflow_values.clone(), params.image_data_urls.clone())
            .await;
        if let Err(e) = &result {
            eprintln!("[comfyui] edit FAILED: {}", e);
        }
        result
    }

    async fn health_check(&self) -> Result<bool, ImagineError> {
        let url = format!("{}/system_stats", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}
