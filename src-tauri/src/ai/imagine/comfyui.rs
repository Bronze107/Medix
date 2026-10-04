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

type ComfyWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

// --- 采样预览 ---
//
// 服务端在每个采样步的回调里把当前 latent 解码成一张 JPEG 推给客户端
// （latent_preview.py 把 preview_format 硬编码为 "JPEG"，最大尺寸 --preview-size，
// 默认 512）。二进制 WS 帧格式为 [4 字节大端事件号][payload]（server.py 的
// encode_bytes），预览相关事件有两种：
//   1 PREVIEW_IMAGE               payload = [4 字节大端类型][图片字节]
//   4 PREVIEW_IMAGE_WITH_METADATA payload = [4 字节大端 JSON 长度][JSON][图片字节]
// 发哪一种取决于客户端是否在连接后声明 supports_preview_metadata。Medix 不声明，
// 实际走事件 1；事件 4 仅为防御性兼容（例如将来声明了该能力）。

/// ComfyUI 二进制事件号（protocol.py 的 BinaryEventTypes）。
const PREVIEW_EVENT_IMAGE: u32 = 1;
const PREVIEW_EVENT_WITH_METADATA: u32 = 4;

/// 预览推送的最小间隔。ComfyUI 每个采样步都会推一帧（25 步 ≈ 25 帧），
/// 全量转发会在几秒内灌出上千 KB 的 IPC，因此限到 ~5fps。
const PREVIEW_MIN_INTERVAL: Duration = Duration::from_millis(200);

/// 推给前端的预览最长边（服务端最大 512px，再缩一道以压低 IPC 体积）。
const PREVIEW_MAX_DIM: u32 = 384;

/// 从一条二进制 WS 帧里取出预览图片字节。
/// 非预览事件、长度不足或空图片一律返回 None —— 绝不 panic（帧来自网络）。
fn parse_preview_frame(frame: &[u8]) -> Option<Vec<u8>> {
    let event = u32::from_be_bytes(frame.get(..4)?.try_into().ok()?);
    let body: &[u8] = match event {
        // 跳过 4 字节图片类型（1=JPEG / 2=PNG），其余为图片本体
        PREVIEW_EVENT_IMAGE => frame.get(8..)?,
        PREVIEW_EVENT_WITH_METADATA => {
            let meta_len = u32::from_be_bytes(frame.get(4..8)?.try_into().ok()?) as usize;
            frame.get(8usize.checked_add(meta_len)?..)?
        }
        _ => return None,
    };
    if body.is_empty() {
        None
    } else {
        Some(body.to_vec())
    }
}

/// 预览图字节 → 缩到最长边 PREVIEW_MAX_DIM 的 JPEG(q80) data URL。
/// 解码失败（截断/非图片）返回 None。
fn preview_data_url(bytes: &[u8]) -> Option<String> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = img.resize(
        PREVIEW_MAX_DIM,
        PREVIEW_MAX_DIM,
        image::imageops::FilterType::Triangle,
    );
    let mut buf = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 80);
    encoder.encode_image(&resized.to_rgb8()).ok()?;
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buf);
    Some(format!("data:image/jpeg;base64,{}", b64))
}

/// 判断 /queue 的响应里是否含有该 prompt。
/// ComfyUI 的 /queue 返回 `{queue_running: [...], queue_pending: [...]}`，
/// 每个条目是 `(number, prompt_id, prompt, extra_data, outputs)` 元组
/// （server.py 的 `_remove_sensitive_from_queue` 取 `item[:5]`），
/// 因此 prompt_id 固定在下标 1。
fn queue_contains(json: &Value, prompt_id: &str) -> bool {
    ["queue_running", "queue_pending"].iter().any(|key| {
        json[*key]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .any(|e| e.get(1).and_then(|v| v.as_str()) == Some(prompt_id))
            })
            .unwrap_or(false)
    })
}

/// 预览推送节流。以局部可变状态在 submit_and_wait → wait_ws 之间传递，
/// 不放进 ComfyuiProvider —— `#[async_trait]` 要求结构体 Sync，Cell 会破坏它。
#[derive(Default)]
struct PreviewThrottle {
    last_sent: Option<Instant>,
}

impl PreviewThrottle {
    /// 距上次发送是否已达最小间隔（只读判断，不改变状态）。
    fn allow(&self, now: Instant) -> bool {
        match self.last_sent {
            Some(last) => now.saturating_duration_since(last) >= PREVIEW_MIN_INTERVAL,
            None => true,
        }
    }

    fn mark_sent(&mut self, now: Instant) {
        self.last_sent = Some(now);
    }
}

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
        // 单次 HTTP 请求的超时跟随「超时(秒)」设置，但不低于 300s：上传源图、
        // 下载结果图都是单次请求，不该被一个很小的采样超时掐断（此前这里硬编码
        // 300s，导致把设置调大后单个请求仍会在 300s 被切断）。
        let request_timeout = Duration::from_secs(timeout_secs.max(300));
        let client = reqwest::Client::builder()
            .timeout(request_timeout)
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
        let object_info = self.fetch_object_info().await;
        // 必须 enrich：inject 依赖 field_type 区分 number/boolean/combo，
        // 而 combo 选项的原始 JSON 类型只有在 object_info 增强后才可还原。
        let params = WorkflowManager::enrich_params(
            &WorkflowManager::parse_params(&self.workflow.workflow_json)
                .map_err(ImagineError::Api)?,
            &object_info,
            &self.workflow.workflow_json,
        );

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
                        if let Some(inputs) = node.get_mut("inputs").and_then(|i| i.as_object_mut())
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

        WorkflowManager::inject(&mut api_prompt, &values, &params, &object_info);

        // 先建立 WebSocket，再提交，避免快速任务在 WS 连接前就完成而错过执行事件。
        let ws = self.connect_ws().await;
        let prompt_id = self.submit(&api_prompt).await?;

        // 登记 prompt_id 供 image_queue_cancel 定向中断。
        if let Some(q) = self.app.try_state::<ImageQueue>() {
            q.set_prompt_id(&self.task_id, prompt_id.clone());
        }

        match ws {
            Ok(mut stream) => {
                let mut throttle = PreviewThrottle::default();
                match self.wait_ws(&mut stream, &prompt_id, &mut throttle).await {
                    Ok(()) => {}
                    Err(ImagineError::WebSocket(msg)) => {
                        eprintln!("[comfyui] ws lost ({}), falling back to polling", msg);
                        self.wait_poll(&prompt_id).await?;
                    }
                    Err(e) => return Err(e),
                }
            }
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
        let mut body = serde_json::json!({
            "prompt": api_prompt,
            "client_id": self.task_id,
        });
        // 采样预览。ComfyUI 的 --preview-method 默认是 none，不会产生任何预览；
        // 但 /prompt 的 extra_data.preview_method 可按 prompt 覆盖
        // （execution.py 的 set_preview_method）。"auto" → Latent2RGB，只需
        // latent format 带 latent_rgb_factors（Qwen Image / Flux 等都有），
        // 不依赖 vae_approx 模型。旧版 ComfyUI 不认识该键时会忽略，无副作用。
        if crate::settings::get_comfyui_preview_enabled(&self.app) {
            body["extra_data"] = serde_json::json!({ "preview_method": "auto" });
        }
        let url = format!("{}/prompt", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
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
            return Err(ImagineError::Api(Self::format_prompt_error(
                &json,
                status.as_u16(),
            )));
        }

        json["prompt_id"].as_str().map(String::from).ok_or_else(|| {
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

    /// 通过 WebSocket 等待执行结果，实时透出进度与采样预览。
    async fn wait_ws(
        &self,
        ws: &mut ComfyWs,
        prompt_id: &str,
        throttle: &mut PreviewThrottle,
    ) -> Result<(), ImagineError> {
        let idle = Duration::from_secs(self.timeout_secs);
        // 空闲超时：只要还在收到 ComfyUI 的帧就续期，真正卡死（服务端不再说话）
        // 才中断。此前是「进入本函数起的固定墙钟预算」，排队等待、加载模型、
        // 长节点都会吃掉它，正常但耗时的工作流会被误杀。
        let mut last_activity = Instant::now();
        loop {
            let remaining = (last_activity + idle).saturating_duration_since(Instant::now());
            let next = match tokio::time::timeout(remaining, ws.next()).await {
                Ok(next) => next,
                Err(_) => {
                    let _ = ws.close(None).await;
                    return Err(ImagineError::Api(format!(
                        "ComfyUI 已 {}s 无响应，任务判定卡死",
                        self.timeout_secs
                    )));
                }
            };
            let Some(frame) = next else { break };
            let frame = frame.map_err(|e| ImagineError::WebSocket(e.to_string()))?;
            // 收到任何一帧都说明服务端还活着（ComfyUI 不会发心跳，帧都是实打实的
            // 进度/预览/状态消息）
            last_activity = Instant::now();

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
                tokio_tungstenite::tungstenite::protocol::Message::Binary(bytes) => {
                    // 采样预览帧。先判节流再做解码：preview_data_url 要做
                    // JPEG 解码 + 缩放 + 重编码，比 should_send 贵得多。
                    let now = Instant::now();
                    if throttle.allow(now) {
                        if let Some(url) = parse_preview_frame(bytes.as_ref())
                            .as_deref()
                            .and_then(preview_data_url)
                        {
                            throttle.mark_sent(now);
                            self.emit_preview(url);
                        }
                    }
                }
                tokio_tungstenite::tungstenite::protocol::Message::Close(_) => break,
                _ => {} // ping/pong 忽略
            }
        }
        Err(ImagineError::WebSocket("连接提前关闭".into()))
    }

    /// WS 连接失败时的兜底：轮询 history。
    /// 注意：这条路拿不到采样预览 —— 预览只走 WebSocket 二进制帧，HTTP
    /// /history 里没有；此路径下前端只会显示进度条。
    async fn wait_poll(&self, prompt_id: &str) -> Result<(), ImagineError> {
        let idle = Duration::from_secs(self.timeout_secs);
        // 与 wait_ws 同样的空闲语义：prompt 只要还在 ComfyUI 的队列里（排队或
        // 执行中）就算活着。不再用固定墙钟预算，否则长任务会在这个兜底路径上
        // 被误杀 —— 而这条路本就只在 WS 连不上时才走。
        let mut last_activity = Instant::now();
        let history_url = format!("{}/history/{}", self.base_url, prompt_id);
        loop {
            if last_activity.elapsed() > idle {
                return Err(ImagineError::Api(format!(
                    "ComfyUI 已 {}s 无响应，任务判定卡死",
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
                            return Err(ImagineError::Api(format!("ComfyUI 执行失败：{}", msg)));
                        }
                        return Ok(());
                    }
                }
            }
            if self.prompt_in_queue(prompt_id).await == Some(true) {
                last_activity = Instant::now();
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// 该 prompt 是否仍在 /queue 的排队或执行列表中。
    /// 返回 None 表示无法判断（接口异常或结构变化），调用方按「不续期」处理，
    /// 使空闲超时仍然生效而不是无限等待。
    async fn prompt_in_queue(&self, prompt_id: &str) -> Option<bool> {
        let url = format!("{}/queue", self.base_url);
        let resp = self.client.get(&url).send().await.ok()?;
        let json: Value = resp.json().await.ok()?;
        Some(queue_contains(&json, prompt_id))
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

        let img_bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &b64_data)
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

    /// 推送一帧采样预览。预览是纯展示态，刻意不入 ImageQueue 的任务状态 ——
    /// 否则 base64 会随 image_queue_list 的返回值反复回传，白白撑爆列表。
    fn emit_preview(&self, data_url: String) {
        let _ = self.app.emit(
            "image-queue-preview",
            serde_json::json!({ "task_id": self.task_id, "data_url": data_url }),
        );
    }

    fn ws_url(base_url: &str, client_id: &str) -> String {
        let scheme = if base_url.starts_with("https") {
            "wss"
        } else {
            "ws"
        };
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
            .submit_and_wait(
                params.workflow_values.clone(),
                params.image_data_urls.clone(),
            )
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    /// 构造一张真实的小 PNG，用作预览帧的图片本体。
    fn tiny_png() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([10, 20, 30]));
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&img, 8, 8, image::ExtendedColorType::Rgb8)
            .unwrap();
        png
    }

    #[test]
    fn test_parse_preview_frame_event1() {
        // [事件 1][图片类型 1 = JPEG][图片字节]
        let mut frame = Vec::new();
        frame.extend_from_slice(&1u32.to_be_bytes());
        frame.extend_from_slice(&1u32.to_be_bytes());
        frame.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0x11, 0x22]);
        assert_eq!(
            parse_preview_frame(&frame),
            Some(vec![0xFF, 0xD8, 0xFF, 0x11, 0x22])
        );
    }

    #[test]
    fn test_parse_preview_frame_with_metadata() {
        // [事件 4][JSON 长度][JSON][图片字节]
        let meta = br#"{"prompt_id":"abc"}"#;
        let mut frame = Vec::new();
        frame.extend_from_slice(&4u32.to_be_bytes());
        frame.extend_from_slice(&(meta.len() as u32).to_be_bytes());
        frame.extend_from_slice(meta);
        frame.extend_from_slice(&[0xFF, 0xD8, 0xFF]);
        assert_eq!(parse_preview_frame(&frame), Some(vec![0xFF, 0xD8, 0xFF]));
    }

    #[test]
    fn test_parse_preview_frame_ignores_other_events() {
        // 事件 3 是 TEXT，事件 2/9 不是预览
        for event in [2u32, 3, 9] {
            let mut frame = event.to_be_bytes().to_vec();
            frame.extend_from_slice(&[1, 2, 3, 4, 5]);
            assert_eq!(parse_preview_frame(&frame), None, "event {event}");
        }
    }

    #[test]
    fn test_parse_preview_frame_truncated_or_empty_does_not_panic() {
        assert_eq!(parse_preview_frame(&[]), None);
        assert_eq!(parse_preview_frame(&[0, 0]), None);
        // 事件 1 但只有类型字段，没有图片本体
        assert_eq!(parse_preview_frame(&[0, 0, 0, 1, 0, 0, 0, 1]), None);
        // 事件 4 声明的 metadata 长度超过帧长
        assert_eq!(
            parse_preview_frame(&[0, 0, 0, 4, 0, 0, 0xFF, 0xFF, 1, 2]),
            None
        );
    }

    #[test]
    fn test_preview_data_url_encodes_jpeg() {
        let url = preview_data_url(&tiny_png()).expect("PNG 应能解码并转成 data URL");
        assert!(url.starts_with("data:image/jpeg;base64,"), "got {url}");
        let b64 = url.strip_prefix("data:image/jpeg;base64,").unwrap();
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
            .expect("payload 应是合法 base64");
        // JPEG 魔数 FF D8 FF
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF]);
    }

    #[test]
    fn test_preview_data_url_rejects_garbage() {
        assert!(preview_data_url(b"definitely not an image").is_none());
        assert!(preview_data_url(&[]).is_none());
    }

    #[test]
    fn test_queue_contains() {
        // /queue 的真实形状：两个数组，条目为 (number, prompt_id, prompt, extra_data, outputs)
        let json = serde_json::json!({
            "queue_running": [[0, "running-id", {}, {}, []]],
            "queue_pending": [[1, "pending-id", {}, {}, []], [2, "other-id", {}, {}, []]]
        });
        assert!(queue_contains(&json, "running-id"));
        assert!(queue_contains(&json, "pending-id"));
        assert!(!queue_contains(&json, "not-here"));
        // 空队列 / 字段缺失 / 结构异常都不能误判为「在里面」
        assert!(!queue_contains(&serde_json::json!({}), "x"));
        assert!(!queue_contains(
            &serde_json::json!({ "queue_running": [], "queue_pending": [] }),
            "x"
        ));
        assert!(!queue_contains(
            &serde_json::json!({ "queue_running": "unexpected" }),
            "x"
        ));
    }

    #[test]
    fn test_preview_throttle() {
        let mut throttle = PreviewThrottle::default();
        let t0 = Instant::now();
        // 第一帧总是放行
        assert!(throttle.allow(t0));
        throttle.mark_sent(t0);
        // 间隔内丢弃
        assert!(!throttle.allow(t0 + Duration::from_millis(50)));
        // 达到最小间隔后放行
        assert!(throttle.allow(t0 + PREVIEW_MIN_INTERVAL));
    }
}
