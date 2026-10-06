use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use chrono::Utc;
use image::ImageEncoder;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Semaphore;

use super::{create_provider, EditParams, GenerateParams, StagedImage};
use crate::media::MediaImportResult;

const MAX_CONCURRENT: usize = 2;

// --- Task types ---

pub enum ImageTask {
    Generate {
        task_id: String,
        prompt: String,
        workflow_values: HashMap<String, String>,
        aspect_ratio: String,
        resolution: String,
        n: u32,
        workflow_id: Option<String>,
    },
    Edit {
        task_id: String,
        source_media_ids: Vec<String>,
        prompt: String,
        workflow_values: HashMap<String, String>,
        /// 前端合成好的源图（裁剪 + 蒙版 alpha）。给出时**取代**从
        /// `source_media_ids` 读文件再编码，长度必须与之对应。
        image_data_urls: Option<Vec<String>>,
        aspect_ratio: String,
        resolution: String,
        n: u32,
        workflow_id: Option<String>,
    },
}

#[derive(Clone, Serialize)]
pub struct TaskProgress {
    pub value: u32,
    pub max: u32,
}

#[derive(Clone, Serialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub task_type: String,
    pub prompt: String,
    pub source_media_ids: Option<Vec<String>>,
    pub status: String,
    pub staged: Vec<StagedImage>,
    pub error: Option<String>,
    pub created_at: String,
    pub progress: Option<TaskProgress>,
}

struct TaskState {
    task_id: String,
    task_type: String,
    prompt: String,
    source_media_ids: Option<Vec<String>>,
    status: String,
    staged: Vec<StagedImage>,
    error: Option<String>,
    created_at: String,
    workflow_id: Option<String>,
    prompt_id: Option<String>,
    progress: Option<TaskProgress>,
}

impl TaskState {
    fn to_info(&self) -> TaskInfo {
        TaskInfo {
            task_id: self.task_id.clone(),
            task_type: self.task_type.clone(),
            prompt: self.prompt.clone(),
            source_media_ids: self.source_media_ids.clone(),
            status: self.status.clone(),
            staged: self.staged.clone(),
            error: self.error.clone(),
            created_at: self.created_at.clone(),
            progress: self.progress.clone(),
        }
    }
}

// --- Queue ---

#[derive(Clone)]
pub struct ImageQueue {
    sender: mpsc::Sender<ImageTask>,
    pending: Arc<AtomicUsize>,
    tasks: Arc<Mutex<HashMap<String, TaskState>>>,
}

impl ImageQueue {
    pub fn send(&self, task: ImageTask) -> Result<(), mpsc::SendError<ImageTask>> {
        self.pending.fetch_add(1, Ordering::SeqCst);
        self.sender.send(task)
    }

    pub fn pending_count(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }

    fn insert_task(&self, state: TaskState) {
        self.tasks
            .lock()
            .unwrap()
            .insert(state.task_id.clone(), state);
    }

    /// 改写任务状态。ComfyUI provider 用它区分「已提交、排在 ComfyUI 自己的队列里」
    /// （`queued`）和「真的开始执行了」（`running`）—— 服务端只有一条执行流水线，
    /// 提交成功 ≠ 开始执行。状态是异步变化的，所以改完要让 provider 顺手 emit。
    pub fn set_status(&self, task_id: &str, status: &str) {
        if let Some(t) = self.tasks.lock().unwrap().get_mut(task_id) {
            t.status = status.to_string();
        }
    }

    fn status_of(&self, task_id: &str) -> Option<String> {
        self.tasks
            .lock()
            .unwrap()
            .get(task_id)
            .map(|t| t.status.clone())
    }

    /// ComfyUI provider 提交后登记 prompt_id，供取消时定向中断。
    pub fn set_prompt_id(&self, task_id: &str, prompt_id: String) {
        if let Some(t) = self.tasks.lock().unwrap().get_mut(task_id) {
            t.prompt_id = Some(prompt_id);
        }
    }

    /// ComfyUI provider 上报采样进度。
    pub fn set_progress(&self, task_id: &str, value: u32, max: u32) {
        if let Some(t) = self.tasks.lock().unwrap().get_mut(task_id) {
            t.progress = Some(TaskProgress { value, max });
        }
    }

    fn get_prompt_id(&self, task_id: &str) -> Option<String> {
        self.tasks
            .lock()
            .unwrap()
            .get(task_id)
            .and_then(|t| t.prompt_id.clone())
    }

    fn list(&self) -> Vec<TaskInfo> {
        let tasks = self.tasks.lock().unwrap();
        let mut list: Vec<TaskInfo> = tasks.values().map(|t| t.to_info()).collect();
        list.sort_by_key(|t| t.created_at.clone());
        list.reverse(); // newest first
        list
    }
}

// --- Init ---

/// 通知前端「任务列表有变化」。前端不轮询列表，收到本事件才会重新拉取
/// `image_queue_list`（见 AiGenPage 的 listen）—— 所以**任何状态写入之后都必须
/// 调一次**。
///
/// 这条规则是被一个真实故障逼出来的：`pending → running` 这个转换原先不发事件，
/// 于是任务明明已经开始跑（进度条和采样预览都在动，它们走的是另一条
/// `image-queue-progress` / `image-queue-preview`），卡片上的文字却一直停在
/// 「排队中」，直到恰好有别的任务结束、顺带刷新了列表才纠正过来。
pub(crate) fn emit_queue_updated(app: &AppHandle) {
    let remaining = app
        .try_state::<ImageQueue>()
        .map(|q| q.pending_count())
        .unwrap_or(0);
    let _ = app.emit(
        "image-queue-updated",
        serde_json::json!({ "remaining": remaining }),
    );
}

pub fn init_image_queue(app: AppHandle) -> ImageQueue {
    let (tx, rx) = mpsc::channel::<ImageTask>();
    let pending = Arc::new(AtomicUsize::new(0));
    let pending_clone = pending.clone();
    let tasks: Arc<Mutex<HashMap<String, TaskState>>> = Arc::new(Mutex::new(HashMap::new()));
    let tasks_clone = tasks.clone();

    let app_clone = app.clone();
    let staging_dir = {
        let app_dir = app.path().app_data_dir().expect("app data dir");
        let staging = app_dir.join("staging");
        fs::create_dir_all(&staging).ok();
        staging
    };

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for image queue");

        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));

        rt.block_on(async move {
            while let Ok(task) = rx.recv() {
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let app = app_clone.clone();
                let pending = pending_clone.clone();
                let tasks = tasks_clone.clone();
                let staging = staging_dir.clone();

                tokio::spawn(async move {
                    let _permit = permit;
                    process_task(app.clone(), &tasks, &staging, task).await;
                    pending.fetch_sub(1, Ordering::SeqCst);
                    emit_queue_updated(&app);
                });
            }
        });
    });

    ImageQueue {
        sender: tx,
        pending,
        tasks,
    }
}

/// 把任务标记为失败。仅用于「构造参数阶段」的失败 —— 那一段在各种分支里
/// 都要做同样的两步（改 status、写 error），散开写很容易漏一处。
fn fail(tasks: &Arc<Mutex<HashMap<String, TaskState>>>, task_id: &str, msg: &str) {
    if let Some(t) = tasks.lock().unwrap().get_mut(task_id) {
        t.status = "failed".to_string();
        t.error = Some(msg.to_string());
    }
}

async fn process_task(
    app: AppHandle,
    tasks: &Arc<Mutex<HashMap<String, TaskState>>>,
    staging_dir: &PathBuf,
    task: ImageTask,
) {
    let (task_id, _task_type, _prompt, generate_params, edit_params, workflow_id) = match task {
        ImageTask::Generate {
            task_id,
            prompt,
            workflow_values,
            aspect_ratio,
            resolution,
            n,
            workflow_id,
        } => {
            let params = GenerateParams {
                prompt: prompt.clone(),
                workflow_values,
                aspect_ratio,
                resolution,
                n,
            };
            (task_id, "generate", prompt, Some(params), None, workflow_id)
        }
        ImageTask::Edit {
            task_id,
            source_media_ids,
            prompt,
            workflow_values,
            image_data_urls: supplied_urls,
            aspect_ratio,
            resolution,
            n,
            workflow_id,
        } => {
            // Resolve all source images for the data URLs (multi-image edit)
            let max_dim: u32 = match resolution.as_str() {
                "2k" => 2048,
                _ => 1024,
            };
            if source_media_ids.is_empty() {
                fail(&tasks, &task_id, "没有选择源图片");
                return;
            }

            let mut image_data_urls = Vec::new();
            match supplied_urls {
                // 前端给了合成好的图（裁剪 + 蒙版）：跳过读文件与按扩展名编码。
                // 长度必须与源图一一对应，否则后续按位置绑定 image_selector 会错位。
                Some(supplied) => {
                    if supplied.len() != source_media_ids.len() {
                        fail(
                            &tasks,
                            &task_id,
                            &format!(
                                "合成图数量({})与源图数量({})不一致",
                                supplied.len(),
                                source_media_ids.len()
                            ),
                        );
                        return;
                    }
                    for url in &supplied {
                        match normalize_supplied_image(url, max_dim) {
                            Ok(u) => image_data_urls.push(u),
                            Err(e) => {
                                fail(&tasks, &task_id, &e);
                                return;
                            }
                        }
                    }
                }
                None => {
                    for sid in &source_media_ids {
                        let path = match crate::db::resolve_media_file(&app, sid) {
                            Ok(p) => p,
                            Err(e) => {
                                fail(&tasks, &task_id, &e);
                                return;
                            }
                        };
                        match read_and_encode_image(&path, max_dim) {
                            Ok(url) => image_data_urls.push(url),
                            Err(e) => {
                                fail(&tasks, &task_id, &e);
                                return;
                            }
                        }
                    }
                }
            }
            let params = EditParams {
                prompt: prompt.clone(),
                workflow_values,
                image_data_urls,
                aspect_ratio,
                resolution,
                n,
            };
            (task_id, "edit", prompt, None, Some(params), workflow_id)
        }
    };

    // Update status to running
    if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
        t.status = "running".to_string();
    }
    // 立刻告知前端 —— 任务可能在这里停留很久（提交 ComfyUI、等它跑完采样），
    // 不发这一条，界面就整段时间都显示「排队中」。
    emit_queue_updated(&app);

    let provider = match create_provider(&app, workflow_id.as_deref(), Some(&task_id)) {
        Ok(p) => p,
        Err(e) => {
            if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
                t.status = "failed".to_string();
                t.error = Some(format!("创建 API 客户端失败: {}", e));
            }
            return;
        }
    };

    let result: Result<Vec<super::GeneratedImage>, super::ImagineError> =
        if let Some(params) = generate_params {
            provider.generate(&params).await
        } else if let Some(params) = edit_params {
            provider.edit(&params).await
        } else {
            return;
        };

    match result {
        Ok(images) => {
            let mut staged_results = Vec::new();
            for img in &images {
                let id = ulid::Ulid::new().to_string();
                let ext = match img.mime_type.as_str() {
                    "image/jpeg" => "jpg",
                    "image/png" => "png",
                    "image/webp" => "webp",
                    _ => "png",
                };
                let temp_path = staging_dir.join(format!("{}.{}", id, ext));
                if let Err(e) = fs::write(&temp_path, &img.data) {
                    eprintln!("[image-queue] failed to write staged image: {}", e);
                    continue;
                }
                let decoded = match image::open(&temp_path) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let file_size = fs::metadata(&temp_path)
                    .map(|m| m.len() as i64)
                    .unwrap_or(0);
                staged_results.push(StagedImage {
                    id,
                    path: temp_path.to_string_lossy().replace('\\', "/"),
                    width: decoded.width() as i32,
                    height: decoded.height() as i32,
                    file_size,
                });
            }

            if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
                t.status = "done".to_string();
                t.staged = staged_results;
            }
        }
        Err(e) => {
            let mut msg = e.to_string();
            let mut src: Option<&dyn std::error::Error> = e.source();
            while let Some(s) = src {
                msg.push_str(&format!("\n  caused by: {}", s));
                src = s.source();
            }
            if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
                t.status = "failed".to_string();
                t.error = Some(msg);
            }
        }
    }
}

/// Read an image from disk, optionally resize, and encode as data URL.
///
/// `source_path` is always a resolved local file (see
/// [`crate::db::resolve_media_file`]), so there is no remote-URL case here.
/// 单张源图编码后的体积上限（base64 字符串长度）。读文件路径与前端合成图
/// 路径共用同一上限，避免两条路各写一个数。
const MAX_BODY: usize = 10 * 1024 * 1024;

fn read_and_encode_image(source_path: &Path, max_dim: u32) -> Result<String, String> {
    let img = image::open(source_path)
        .map_err(|e| format!("无法读取源图片 {}：{}", source_path.to_string_lossy(), e))?;
    let (w, h) = (img.width(), img.height());
    let image_data_url = if w.max(h) > max_dim {
        let ratio = max_dim as f64 / w.max(h) as f64;
        let new_w = (w as f64 * ratio).round() as u32;
        let new_h = (h as f64 * ratio).round() as u32;
        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        image_to_data_url(&resized, source_path)?
    } else {
        image_to_data_url(&img, source_path)?
    };

    let b64_len = image_data_url.len();
    if b64_len > MAX_BODY {
        return Err(format!(
            "Image too large after encoding ({}MB > 10MB limit). Try a lower resolution.",
            b64_len / (1024 * 1024)
        ));
    }

    Ok(image_data_url)
}

/// 前端合成的源图（data URL）→ 归一化后的 data URL。
///
/// **不能走 `image_to_data_url`**：那个按源文件扩展名选编码格式，而 data URL
/// 没有扩展名。这里一律重编码为 PNG/RGBA —— 蒙版就藏在 alpha 通道里，转成
/// JPEG 会把它整个丢掉（重绘就会变成整图重绘或完全不重绘）。
pub fn normalize_supplied_image(data_url: &str, max_dim: u32) -> Result<String, String> {
    let (mime_ok, b64) = match data_url.find(',') {
        Some(i) => (data_url[..i].starts_with("data:image/"), &data_url[i + 1..]),
        None => return Err("无效的图片 data URL".to_string()),
    };
    if !mime_ok {
        return Err("无效的图片 data URL".to_string());
    }
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
        .map_err(|e| format!("base64 解码失败：{}", e))?;
    let img = image::load_from_memory(&bytes).map_err(|e| format!("无法解码合成图：{}", e))?;

    let (w, h) = (img.width(), img.height());
    let img = if max_dim > 0 && w.max(h) > max_dim {
        let scale = max_dim as f64 / w.max(h) as f64;
        img.resize_exact(
            ((w as f64 * scale).round() as u32).max(1),
            ((h as f64 * scale).round() as u32).max(1),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };

    let rgba = img.to_rgba8();
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(
            &rgba,
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;

    let out = format!(
        "data:image/png;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buf)
    );
    // 沿用与读文件路径相同的体积上限
    if out.len() > MAX_BODY {
        return Err(format!(
            "合成图过大（{}MB > 10MB），请降低分辨率",
            out.len() / (1024 * 1024)
        ));
    }
    Ok(out)
}

fn image_to_data_url(img: &image::DynamicImage, source_path: &Path) -> Result<String, String> {
    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();

    let (mime, bytes) = match ext.as_str() {
        "png" => {
            let rgba = img.to_rgba8();
            let mut buf = Vec::new();
            let encoder = image::codecs::png::PngEncoder::new(&mut buf);
            encoder
                .write_image(
                    &rgba,
                    img.width(),
                    img.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| e.to_string())?;
            ("image/png", buf)
        }
        _ => {
            let mut buf = Vec::new();
            let rgb = img.to_rgb8();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85);
            encoder.encode_image(&rgb).map_err(|e| e.to_string())?;
            ("image/jpeg", buf)
        }
    };

    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

// --- Tauri Commands ---

#[tauri::command]
pub fn image_queue_submit_generate(
    app: AppHandle,
    prompt: String,
    workflow_values: Option<HashMap<String, String>>,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    n: Option<u32>,
    workflow_id: Option<String>,
) -> Result<String, String> {
    let queue = app.state::<ImageQueue>();
    let task_id = ulid::Ulid::new().to_string();
    let task = ImageTask::Generate {
        task_id: task_id.clone(),
        prompt: prompt.trim().to_string(),
        workflow_values: workflow_values.unwrap_or_default(),
        aspect_ratio: aspect_ratio.unwrap_or_else(|| "auto".to_string()),
        resolution: resolution.unwrap_or_else(|| "1k".to_string()),
        n: n.unwrap_or(1),
        workflow_id: workflow_id.clone(),
    };
    queue.insert_task(TaskState {
        task_id: task_id.clone(),
        task_type: "generate".to_string(),
        prompt: prompt.trim().to_string(),
        source_media_ids: None,
        status: "pending".to_string(),
        staged: Vec::new(),
        error: None,
        created_at: Utc::now().to_rfc3339(),
        workflow_id,
        prompt_id: None,
        progress: None,
    });
    queue.send(task).map_err(|e| e.to_string())?;
    emit_queue_updated(&app);
    Ok(task_id)
}

/// 提交图生图任务。
///
/// `image_data_urls` 是编辑器合成好的源图（裁剪 + 蒙版 alpha）：给出时取代
/// 「按 id 读文件再编码」的路径，数量须与 `source_media_ids` 一致。
#[tauri::command]
pub fn image_queue_submit_edit(
    app: AppHandle,
    source_media_ids: Vec<String>,
    prompt: String,
    workflow_values: Option<HashMap<String, String>>,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    n: Option<u32>,
    workflow_id: Option<String>,
    image_data_urls: Option<Vec<String>>,
) -> Result<String, String> {
    let queue = app.state::<ImageQueue>();
    let task_id = ulid::Ulid::new().to_string();
    let task = ImageTask::Edit {
        task_id: task_id.clone(),
        source_media_ids: source_media_ids.clone(),
        prompt: prompt.trim().to_string(),
        workflow_values: workflow_values.unwrap_or_default(),
        image_data_urls,
        aspect_ratio: aspect_ratio.unwrap_or_else(|| "auto".to_string()),
        resolution: resolution.unwrap_or_else(|| "1k".to_string()),
        n: n.unwrap_or(1),
        workflow_id: workflow_id.clone(),
    };
    queue.insert_task(TaskState {
        task_id: task_id.clone(),
        task_type: "edit".to_string(),
        prompt: prompt.trim().to_string(),
        source_media_ids: Some(source_media_ids),
        status: "pending".to_string(),
        staged: Vec::new(),
        error: None,
        created_at: Utc::now().to_rfc3339(),
        workflow_id,
        prompt_id: None,
        progress: None,
    });
    queue.send(task).map_err(|e| e.to_string())?;
    emit_queue_updated(&app);
    Ok(task_id)
}

#[tauri::command]
pub fn image_queue_list(app: AppHandle) -> Vec<TaskInfo> {
    app.state::<ImageQueue>().list()
}

#[tauri::command]
pub fn image_queue_pending_count(app: AppHandle) -> u32 {
    app.state::<ImageQueue>().pending_count() as u32
}

/// 把任务的暂存产物导入为正式媒体。
///
/// 三件事决定了它的形状：
///
/// 1. **不占主线程**。同步 command 是在主线程上执行的，而这个函数要复制文件、
///    解码、缩放、生成缩略图、写库 —— 留在主线程就等于整个窗口卡住。所以做成
///    async command + `spawn_blocking`。（`#[tauri::command(async)]` 不够：它只是
///    把函数体塞进一个 async 块，仍然占着 async 运行时的工作线程。）
/// 2. **不占任务表的锁**。只在「取快照」和「写回」两处短暂加锁，中间的重活一律
///    在锁外。否则 `image_queue_list`（同步 command，跑在主线程）会在锁上干等，
///    运行中任务的 `set_progress` 与状态写入也会被一起挡住。
/// 3. 与 2 配套：放锁之后 `image_queue_discard` 可能在这期间删掉暂存文件，所以
///    单张失败不能再 `?` 掉整批。
#[tauri::command]
pub async fn image_queue_import(
    app: AppHandle,
    task_id: String,
    selected_ids: Vec<String>,
) -> Result<Vec<MediaImportResult>, String> {
    // ImageQueue 里只有一个 mpsc::Sender 和两个 Arc，clone 出来的是指向同一队列的
    // 句柄。必须拿到所有权：State<'_, _> 是借用，活不过 spawn_blocking 的 'static 闭包。
    let queue = ImageQueue::clone(app.state::<ImageQueue>().inner());
    tauri::async_runtime::spawn_blocking(move || {
        import_staged(&app, &queue, &task_id, &selected_ids)
    })
    .await
    .map_err(|e| format!("导入失败：{}", e))?
}

fn import_staged(
    app: &AppHandle,
    queue: &ImageQueue,
    task_id: &str,
    selected_ids: &[String],
) -> Result<Vec<MediaImportResult>, String> {
    // ── 锁内：只取一份快照 ─────────────────────────────────────────────
    let (prompt, workflow_id, source_media_ids, selected) = {
        let tasks = queue.tasks.lock().unwrap();
        let task = tasks.get(task_id).ok_or("Task not found")?;
        if selected_ids.is_empty() {
            return Ok(Vec::new());
        }
        let selected: Vec<StagedImage> = task
            .staged
            .iter()
            .filter(|s| selected_ids.contains(&s.id))
            .cloned()
            .collect();
        (
            task.prompt.clone(),
            task.workflow_id.clone(),
            task.source_media_ids.clone(),
            selected,
        )
    };

    if selected.is_empty() {
        return Ok(Vec::new());
    }

    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let staging_dir = app_dir.join("staging");
    let library_dir = app_dir.join("library");

    // ── 锁外：复制、解码、写库、生成缩略图 ──────────────────────────────
    let results = match source_media_ids {
        Some(ids) if !ids.is_empty() => import_as_derivatives(
            app,
            &staging_dir,
            &library_dir,
            &selected,
            &ids,
            &prompt,
            workflow_id.as_deref(),
        ),
        Some(_) => return Err("No source media IDs provided for edit task".to_string()),
        None => import_as_new_media(app, &staging_dir, &library_dir, &selected, &prompt),
    };

    // ── 锁内：摘掉已导入的暂存图；全摘完就把任务整个删掉 ────────────────
    let mut tasks = queue.tasks.lock().unwrap();
    let mut empty = false;
    if let Some(task) = tasks.get_mut(task_id) {
        let selected_set: HashSet<&String> = selected_ids.iter().collect();
        task.staged.retain(|s| !selected_set.contains(&s.id));
        empty = task.staged.is_empty();
    }
    if empty {
        tasks.remove(task_id);
    }

    Ok(results)
}

/// 一次失败的导入结果（`path` 用暂存路径，便于定位是哪一张）。
fn import_failed(path: &str, error: String) -> MediaImportResult {
    MediaImportResult {
        id: String::new(),
        path: path.to_string(),
        success: false,
        error: Some(error),
    }
}

/// 暂存文件 → library 副本 + 解码结果。两个导入分支的这一步完全一样。
struct StagedFile {
    dest: PathBuf,
    dest_str: String,
    decoded: image::DynamicImage,
    file_size: i64,
}

/// 复制进 `library/` 并解码。失败时返回给用户看的错误（调用方记一条失败结果、
/// 跳到下一张 —— 见 `image_queue_import` 的第 3 条说明）。
fn copy_and_decode(
    staging_dir: &Path,
    library_dir: &Path,
    img: &StagedImage,
    new_id: &str,
) -> Result<StagedFile, String> {
    let ext = find_staged_ext(staging_dir, &img.id)?;
    let src = staging_dir.join(format!("{}.{}", img.id, ext));
    let dest = library_dir.join(format!("{}.{}", new_id, ext));

    fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    // 副本已经进 library，暂存文件删掉（失败也无所谓，下次不会再找到它）
    let _ = fs::remove_file(&src);

    let decoded = image::open(&dest).map_err(|e| e.to_string())?;
    let file_size = fs::metadata(&dest).map(|m| m.len() as i64).unwrap_or(0);
    let dest_str = dest.to_string_lossy().replace('\\', "/");
    Ok(StagedFile {
        dest,
        dest_str,
        decoded,
        file_size,
    })
}

/// 编辑任务的产物：每个源图各建一个新版本（新 media 行 + 到所有源图的 lineage 边）。
fn import_as_derivatives(
    app: &AppHandle,
    staging_dir: &Path,
    library_dir: &Path,
    selected: &[StagedImage],
    source_media_ids: &[String],
    prompt: &str,
    workflow_id: Option<&str>,
) -> Vec<MediaImportResult> {
    // ComfyUI 任务必有 workflow_id，xAI 任务没有 → 据此打来源标签。
    let provider_tag = if workflow_id.is_some() {
        "comfyui"
    } else {
        "xai"
    };
    let source = format!("ai-edited:{}", provider_tag);

    let mut results = Vec::new();
    for img in selected {
        let new_id = ulid::Ulid::new().to_string();
        // Named after the derivative's own id, like every other media file.
        // The old "{parent}_{child}" form meant a derivative's file did not
        // start with its own id, so deleting it never found the file.
        let file = match copy_and_decode(staging_dir, library_dir, img, &new_id) {
            Ok(f) => f,
            Err(e) => {
                results.push(import_failed(&img.path, e));
                continue;
            }
        };

        let media = crate::media::Media {
            id: new_id.clone(),
            source_path: Some(file.dest_str.clone()),
            width: Some(file.decoded.width() as i32),
            height: Some(file.decoded.height() as i32),
            file_size: Some(file.file_size),
            created_at: None,
            modified_at: None,
            imported_at: Utc::now().to_rfc3339(),
            source_url: None,
            page_url: None,
            source: Some(source.clone()),
            phash: None,
            sha256: None,
            deleted_at: None,
            display_variant_id: None,
            thumb_256: None,
            lqip: None,
            media_type: Some("image".to_string()),
            duration: None,
            video_codec: None,
            video_fps: None,
        };

        if let Err(e) = crate::db::insert_media(app, &media) {
            let _ = fs::remove_file(&file.dest);
            results.push(import_failed(&img.path, e.to_string()));
            continue;
        }

        // Create lineage links for ALL source images
        for src_id in source_media_ids {
            if let Err(e) = crate::db::lineage_insert(app, src_id, &new_id, "edit", workflow_id) {
                eprintln!("[image-queue] failed to insert lineage: {}", e);
            }
        }

        // Generate thumbnail
        if let Err(e) =
            crate::media::thumbnail::generate_thumbnails_from_image(app, &new_id, &file.decoded)
        {
            eprintln!("[image-queue] thumbnail failed: {}", e);
        }

        // Save prompt as caption (if not empty)
        if !prompt.is_empty() {
            if let Err(e) =
                crate::db::caption_create_with_source(app, &new_id, prompt, Some("ai-edit"))
            {
                eprintln!("[image-queue] failed to save prompt caption: {}", e);
            }
        }

        results.push(MediaImportResult {
            id: new_id,
            path: file.dest_str,
            success: true,
            error: None,
        });
    }
    results
}

/// 文生图任务的产物：每张各建一条新 media 行，并触发 AI 标注。
fn import_as_new_media(
    app: &AppHandle,
    staging_dir: &Path,
    library_dir: &Path,
    selected: &[StagedImage],
    prompt: &str,
) -> Vec<MediaImportResult> {
    let source = format!("generated:{}", crate::settings::get_image_api_provider(app));

    let mut results = Vec::new();
    for img in selected {
        let id = ulid::Ulid::new().to_string();
        let file = match copy_and_decode(staging_dir, library_dir, img, &id) {
            Ok(f) => f,
            Err(e) => {
                results.push(import_failed(&img.path, e));
                continue;
            }
        };

        let lqip = {
            let data_url = crate::media::thumbnail::generate_lqip(&file.decoded);
            if data_url.is_empty() {
                None
            } else {
                Some(data_url)
            }
        };

        let media = crate::media::Media {
            id: id.clone(),
            source_path: None,
            width: Some(file.decoded.width() as i32),
            height: Some(file.decoded.height() as i32),
            file_size: Some(file.file_size),
            created_at: None,
            modified_at: None,
            imported_at: Utc::now().to_rfc3339(),
            source_url: None,
            page_url: None,
            source: Some(source.clone()),
            phash: None,
            sha256: None,
            deleted_at: None,
            display_variant_id: None,
            thumb_256: None,
            lqip,
            media_type: None,
            duration: None,
            video_codec: None,
            video_fps: None,
        };

        if let Err(e) = crate::db::insert_media(app, &media) {
            let _ = fs::remove_file(&file.dest);
            results.push(import_failed(&img.path, e.to_string()));
            continue;
        }

        // Generate thumbnails —— 另起线程，不让导入的返回路径等它
        let img_clone = file.decoded.clone();
        let app_clone = app.clone();
        let mid = id.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::media::thumbnail::generate_thumbnails_from_image(
                &app_clone, &mid, &img_clone,
            ) {
                eprintln!("[image-queue] thumbnail failed: {}", e);
            }
        });

        // Save prompt as caption (skip if empty — ComfyUI workflows may not expose a text param)
        if !prompt.is_empty() {
            if let Err(e) =
                crate::db::caption_create_with_source(app, &id, prompt, Some("ai-generated"))
            {
                eprintln!("[image-queue] failed to save prompt caption: {}", e);
            }
        }

        // Trigger AI annotation
        let app_clone = app.clone();
        let mid = id.clone();
        let dest_clone = file.dest.clone();
        std::thread::spawn(move || {
            let queue = app_clone.state::<crate::ai::AiQueue>();
            let _ = queue.send(crate::ai::AiTask::GenerateCaption {
                media_id: mid,
                image_path: dest_clone,
            });
        });

        results.push(MediaImportResult {
            id,
            path: img.path.clone(),
            success: true,
            error: None,
        });
    }
    results
}

#[tauri::command]
pub fn image_queue_discard(app: AppHandle, task_id: String) -> Result<(), String> {
    let queue = app.state::<ImageQueue>();
    let staging_dir = {
        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        app_dir.join("staging")
    };

    let mut tasks = queue.tasks.lock().unwrap();
    if let Some(task) = tasks.get(&task_id) {
        for img in &task.staged {
            if let Ok(ext) = find_staged_ext(&staging_dir, &img.id) {
                let _ = fs::remove_file(staging_dir.join(format!("{}.{}", img.id, ext)));
            }
        }
    }
    tasks.remove(&task_id);
    Ok(())
}

#[tauri::command]
pub fn image_queue_dismiss(app: AppHandle, task_id: String) -> Result<(), String> {
    // Just remove from list, keep files on disk (may be imported later)
    app.state::<ImageQueue>()
        .tasks
        .lock()
        .unwrap()
        .remove(&task_id);
    Ok(())
}

/// 取消一个已提交给 ComfyUI 的任务。
///
/// 两个状态要走两个不同的接口，因为服务端的 `/interrupt` 对**没在跑**的 prompt
/// 是静默 no-op（server.py 里只写一行 skipping 日志然后返回 200）：
///
/// * `running` → `POST /interrupt {"prompt_id"}`；服务端随后回
///   `execution_interrupted`，provider 靠它收尾。
/// * `queued`  → `POST /queue {"delete": [prompt_id]}`（即 server.py 里 `dequeue`
///   的做法）。被删掉的 prompt 服务端**不会**发任何专属事件，只广播一条
///   `status`；provider 靠那条消息去查 /queue 才发现自己没了（见 comfyui.rs
///   的 wait_ws）。这一点很关键：不然取消排队的任务会静静地什么都不发生，
///   而那个任务还占着 Medix 的一个并发名额。
#[tauri::command]
pub async fn image_queue_cancel(app: AppHandle, task_id: String) -> Result<String, String> {
    let queue = app.state::<ImageQueue>();
    let prompt_id = queue
        .get_prompt_id(&task_id)
        .ok_or("任务不存在或尚未提交到 ComfyUI")?;
    let status = queue.status_of(&task_id).ok_or("任务不存在")?;
    if status != "running" && status != "queued" {
        return Err("任务未在运行".into());
    }

    let base_url = crate::settings::get_comfyui_base_url(&app);
    let client = reqwest::Client::new();

    if status == "queued" {
        let resp = client
            .post(format!("{}/queue", base_url))
            .json(&serde_json::json!({ "delete": [prompt_id] }))
            .send()
            .await
            .map_err(|e| format!("移除排队任务失败：{}", e))?;
        if !resp.status().is_success() {
            return Err(format!("ComfyUI 拒绝移除排队任务 (HTTP {})", resp.status()));
        }
        // 从上面读状态到这次删除之间，它可能刚好轮到并开始执行 —— 那样这次删除
        // 是空操作，得改用中断，否则用户以为取消了、任务却还在跑。
        if super::comfyui::fetch_queue_state(&client, &base_url, &prompt_id).await
            == super::comfyui::QueueState::Running
        {
            return interrupt(&client, &base_url, &prompt_id).await;
        }
        return Ok("已从 ComfyUI 队列移除".to_string());
    }

    interrupt(&client, &base_url, &prompt_id).await
}

/// `POST /interrupt`，中断正在执行的那个 prompt（服务端只在 prompt_id 确实是
/// 当前运行项时才真的打断）。
async fn interrupt(
    client: &reqwest::Client,
    base_url: &str,
    prompt_id: &str,
) -> Result<String, String> {
    let resp = client
        .post(format!("{}/interrupt", base_url))
        .json(&serde_json::json!({ "prompt_id": prompt_id }))
        .send()
        .await
        .map_err(|e| format!("中断请求失败：{}", e))?;
    let status = resp.status();
    if status.is_success() {
        Ok("已发送中断请求".to_string())
    } else {
        Err(format!("ComfyUI 中断请求失败 (HTTP {})", status))
    }
}

fn find_staged_ext(staging: &std::path::Path, id: &str) -> Result<String, String> {
    for ext in &["jpg", "jpeg", "png", "webp"] {
        let path = staging.join(format!("{}.{}", id, ext));
        if path.exists() {
            return Ok(ext.to_string());
        }
    }
    Err(format!("Staged file not found for {}", id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    /// 造一张 RGBA PNG 的 data URL：左上角 alpha=0（即要被重绘的区域）。
    fn rgba_png_data_url(w: u32, h: u32) -> String {
        let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        img.put_pixel(0, 0, image::Rgba([10, 20, 30, 0]));
        let mut buf = Vec::new();
        image::codecs::png::PngEncoder::new(&mut buf)
            .write_image(&img, w, h, image::ExtendedColorType::Rgba8)
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &buf)
        )
    }

    fn decode(url: &str) -> image::DynamicImage {
        let b64 = &url[url.find(',').unwrap() + 1..];
        let bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64).unwrap();
        image::load_from_memory(&bytes).unwrap()
    }

    #[test]
    fn test_normalize_supplied_image_preserves_alpha() {
        // 关键性质：蒙版藏在 alpha 里，归一化必须保留通道 —— 一旦转成 JPEG
        // （alpha 被丢弃），局部重绘就会静默退化成整图重绘。
        let url = rgba_png_data_url(8, 8);
        let out = normalize_supplied_image(&url, 1024).unwrap();
        assert!(out.starts_with("data:image/png;base64,"));

        let img = decode(&out).to_rgba8();
        assert_eq!(img.width(), 8);
        assert_eq!(img.get_pixel(0, 0)[3], 0, "被涂抹处 alpha 应保持 0");
        assert_eq!(img.get_pixel(3, 3)[3], 255, "其余区域应为不透明");
    }

    #[test]
    fn test_normalize_supplied_image_downscales_to_max_dim() {
        let out = normalize_supplied_image(&rgba_png_data_url(400, 200), 100).unwrap();
        let img = decode(&out);
        assert_eq!(img.width().max(img.height()), 100);
        // 比例保持（允许取整误差）
        assert!((img.width() as f64 / img.height() as f64 - 2.0).abs() < 0.05);
    }

    #[test]
    fn test_normalize_supplied_image_rejects_garbage() {
        assert!(normalize_supplied_image("not a data url", 1024).is_err());
        assert!(normalize_supplied_image("data:text/plain;base64,AAAA", 1024).is_err());
        assert!(normalize_supplied_image("data:image/png;base64,bm90YW5pbWFnZQ==", 1024).is_err());
    }
}
