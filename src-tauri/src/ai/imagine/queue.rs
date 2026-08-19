use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use chrono::Utc;
use image::ImageEncoder;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Semaphore;

use rusqlite::params;

use super::{create_provider, EditParams, GenerateParams, StagedImage};

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
        self.tasks.lock().unwrap().insert(state.task_id.clone(), state);
    }

    fn update_status(&self, task_id: &str, status: &str, error: Option<String>) {
        if let Some(t) = self.tasks.lock().unwrap().get_mut(task_id) {
            t.status = status.to_string();
            t.error = error;
        }
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

    fn set_staged(&self, task_id: &str, staged: Vec<StagedImage>) {
        if let Some(t) = self.tasks.lock().unwrap().get_mut(task_id) {
            t.staged = staged;
        }
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

pub fn init_image_queue(app: AppHandle) -> ImageQueue {
    let (tx, rx) = mpsc::channel::<ImageTask>();
    let pending = Arc::new(AtomicUsize::new(0));
    let pending_clone = pending.clone();
    let tasks: Arc<Mutex<HashMap<String, TaskState>>> = Arc::new(Mutex::new(HashMap::new()));
    let tasks_clone = tasks.clone();

    let app_clone = app.clone();
    let staging_dir = {
        let app_dir = app
            .path()
            .app_data_dir()
            .expect("app data dir");
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

                let app2 = app_clone.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    process_task(app, &tasks, &staging, task).await;
                    let remaining = pending.fetch_sub(1, Ordering::SeqCst) - 1;
                    let _ = app2.emit(
                        "image-queue-updated",
                        serde_json::json!({ "remaining": remaining }),
                    );
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
            let conn = match crate::db::get_conn(&app) {
                Ok(c) => c,
                Err(e) => {
                    if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
                        t.status = "failed".to_string();
                        t.error = Some(e);
                    }
                    return;
                }
            };
            if source_media_ids.is_empty() {
                if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
                    t.status = "failed".to_string();
                    t.error = Some("没有选择源图片".to_string());
                }
                return;
            }
            let mut image_data_urls = Vec::new();
            for sid in &source_media_ids {
                let path = match resolve_media_source(&app, &*conn, sid) {
                    Ok(p) => p,
                    Err(e) => {
                        if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
                            t.status = "failed".to_string();
                            t.error = Some(e);
                        }
                        return;
                    }
                };
                match read_and_encode_image(&path, max_dim) {
                    Ok(url) => image_data_urls.push(url),
                    Err(e) => {
                        if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
                            t.status = "failed".to_string();
                            t.error = Some(e);
                        }
                        return;
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
            (
                task_id,
                "edit",
                prompt,
                None,
                Some(params),
                workflow_id,
            )
        }
    };

    // Update status to running
    if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
        t.status = "running".to_string();
    }

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

    let result: Result<Vec<super::GeneratedImage>, super::ImagineError> = if let Some(params) = generate_params
    {
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
                let file_size =
                    fs::metadata(&temp_path).map(|m| m.len() as i64).unwrap_or(0);
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

/// Look up the source_path from the media table.
fn find_media_path(conn: &rusqlite::Connection, media_id: &str) -> Result<String, String> {
    let path: String = conn
        .query_row(
            "SELECT source_path FROM media WHERE id = ?1",
            params![media_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("media {} not found: {}", media_id, e))?;
    Ok(path)
}

/// source_path 为网络链接时不能作为本地文件读取（浏览器插件导入的 Web 图片）。
fn is_remote_url(path: &str) -> bool {
    path.starts_with("http://")
        || path.starts_with("https://")
        || path.starts_with("asset://")
        || path.starts_with("file://")
}

/// 解析媒体实际本地文件路径。
/// 历史 Web 导入记录的 source_path 是 URL，但文件实际在 library/{id}.{ext}，
/// 这里回退到按 media id 前缀在 library 目录中查找真实文件。
fn resolve_media_source(
    app: &AppHandle,
    conn: &rusqlite::Connection,
    media_id: &str,
) -> Result<String, String> {
    let path = find_media_path(conn, media_id)?;
    if !is_remote_url(&path) {
        return Ok(path);
    }
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library_dir = app_dir.join("library");
    let prefix = format!("{}.", media_id);
    let entries = std::fs::read_dir(&library_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) {
            return Ok(entry.path().to_string_lossy().replace('\\', "/"));
        }
    }
    Err(format!(
        "该媒体是网络链接（{}）且未在本地 library 中找到文件，无法用于本地图像编辑",
        path
    ))
}

/// Read an image from disk, optionally resize, and encode as data URL.
fn read_and_encode_image(source_path: &str, max_dim: u32) -> Result<String, String> {
    if is_remote_url(source_path) {
        return Err(format!(
            "该媒体是网络链接（source_path: {}），没有本地文件，无法用于本地图像编辑。\
             请先将其下载到本地再操作。",
            source_path
        ));
    }
    let img = image::open(source_path)
        .map_err(|e| format!("无法读取源图片 {}：{}", source_path, e))?;
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
    const MAX_BODY: usize = 10 * 1024 * 1024;
    if b64_len > MAX_BODY {
        return Err(format!(
            "Image too large after encoding ({}MB > 10MB limit). Try a lower resolution.",
            b64_len / (1024 * 1024)
        ));
    }

    Ok(image_data_url)
}

fn image_to_data_url(img: &image::DynamicImage, source_path: &str) -> Result<String, String> {
    let ext = std::path::Path::new(source_path)
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
                .write_image(&rgba, img.width(), img.height(), image::ExtendedColorType::Rgba8)
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
    let _ = app.emit("image-queue-updated", serde_json::json!({ "remaining": queue.pending_count() }));
    Ok(task_id)
}

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
) -> Result<String, String> {
    let queue = app.state::<ImageQueue>();
    let task_id = ulid::Ulid::new().to_string();
    let task = ImageTask::Edit {
        task_id: task_id.clone(),
        source_media_ids: source_media_ids.clone(),
        prompt: prompt.trim().to_string(),
        workflow_values: workflow_values.unwrap_or_default(),
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
    let _ = app.emit("image-queue-updated", serde_json::json!({ "remaining": queue.pending_count() }));
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

#[tauri::command]
pub fn image_queue_import(
    app: AppHandle,
    task_id: String,
    selected_ids: Vec<String>,
) -> Result<Vec<crate::media::MediaImportResult>, String> {
    let queue = app.state::<ImageQueue>();
    let mut tasks = queue.tasks.lock().unwrap();
    let task = tasks.get_mut(&task_id).ok_or("Task not found")?;

    if selected_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Filter selected staged images
    let selected: Vec<&StagedImage> = task
        .staged
        .iter()
        .filter(|s| selected_ids.contains(&s.id))
        .collect();

    if selected.is_empty() {
        return Ok(Vec::new());
    }

    let staging_dir = {
        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        app_dir.join("staging")
    };

    let provider = crate::settings::get_image_api_provider(&app);

    if let Some(ref source_media_ids) = task.source_media_ids {
        // Edit mode: create derivative media records with lineage
        if source_media_ids.is_empty() {
            return Err("No source media IDs provided for edit task".to_string());
        }

        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let library_dir = app_dir.join("library");

        // ComfyUI 任务必有 workflow_id，xAI 任务没有 → 据此打来源标签。
        let provider_tag = if task.workflow_id.is_some() { "comfyui" } else { "xai" };
        let source = format!("ai-edited:{}", provider_tag);

        let mut results = Vec::new();
        for img in &selected {
            let ext = find_staged_ext(&staging_dir, &img.id)?;
            let src = staging_dir.join(format!("{}.{}", img.id, ext));
            let new_id = ulid::Ulid::new().to_string();
            let dest = library_dir.join(format!("{}_{}.{}", source_media_ids[0], new_id, ext));

            if let Err(e) = fs::copy(&src, &dest) {
                results.push(crate::media::MediaImportResult {
                    id: String::new(),
                    path: img.path.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
                continue;
            }
            let _ = fs::remove_file(&src);

            let decoded = match image::open(&dest) {
                Ok(d) => d,
                Err(e) => {
                    results.push(crate::media::MediaImportResult {
                        id: String::new(),
                        path: img.path.clone(),
                        success: false,
                        error: Some(e.to_string()),
                    });
                    continue;
                }
            };
            let file_size = fs::metadata(&dest).map(|m| m.len() as i64).unwrap_or(0);
            let dest_str = dest.to_string_lossy().replace('\\', "/");

            let media = crate::media::Media {
                id: new_id.clone(),
                source_path: Some(dest_str.clone()),
                width: Some(decoded.width() as i32),
                height: Some(decoded.height() as i32),
                file_size: Some(file_size),
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

            if let Err(e) = crate::db::insert_media(&app, &media) {
                let _ = fs::remove_file(&dest);
                results.push(crate::media::MediaImportResult {
                    id: String::new(),
                    path: img.path.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
                continue;
            }

            // Create lineage links for ALL source images
            for src_id in source_media_ids {
                if let Err(e) = crate::db::lineage_insert(&app, src_id, &new_id, "edit", task.workflow_id.as_deref()) {
                    eprintln!("[image-queue] failed to insert lineage: {}", e);
                }
            }

            // Generate thumbnail
            if let Err(e) = crate::media::thumbnail::generate_thumbnails_from_image(&app, &new_id, &decoded) {
                eprintln!("[image-queue] thumbnail failed: {}", e);
            }

            // Save prompt as caption (if not empty)
            if !task.prompt.is_empty() {
                if let Err(e) = crate::db::caption_create_with_source(&app, &new_id, &task.prompt, Some("ai-edit")) {
                    eprintln!("[image-queue] failed to save prompt caption: {}", e);
                }
            }

            results.push(crate::media::MediaImportResult {
                id: new_id,
                path: dest_str,
                success: true,
                error: None,
            });
        }
        // Remove imported staged images from task
        let selected_set: std::collections::HashSet<_> = selected_ids.iter().collect();
        task.staged.retain(|s| !selected_set.contains(&s.id));
        // If all staged images imported, remove the task entirely
        if task.staged.is_empty() {
            drop(tasks); // release lock before mutation in remove
            queue.tasks.lock().unwrap().remove(&task_id);
        }
        Ok(results)
    } else {
        // Generate mode: import as new media
        let source = format!("generated:{}", provider);
        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let library_dir = app_dir.join("library");

        let mut results = Vec::new();
        for img in &selected {
            let ext = find_staged_ext(&staging_dir, &img.id)?;
            let src = staging_dir.join(format!("{}.{}", img.id, ext));
            let id = ulid::Ulid::new().to_string();
            let dest = library_dir.join(format!("{}.{}", id, ext));

            if let Err(e) = fs::copy(&src, &dest) {
                results.push(crate::media::MediaImportResult {
                    id: String::new(),
                    path: img.path.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
                continue;
            }
            let _ = fs::remove_file(&src);

            let decoded = match image::open(&dest) {
                Ok(d) => d,
                Err(e) => {
                    results.push(crate::media::MediaImportResult {
                        id: String::new(),
                        path: img.path.clone(),
                        success: false,
                        error: Some(e.to_string()),
                    });
                    continue;
                }
            };
            let file_size = fs::metadata(&dest).map(|m| m.len() as i64).unwrap_or(0);

            let lqip = {
                let data_url = crate::media::thumbnail::generate_lqip(&decoded);
                if data_url.is_empty() { None } else { Some(data_url) }
            };

            let media = crate::media::Media {
                id: id.clone(),
                source_path: None,
                width: Some(decoded.width() as i32),
                height: Some(decoded.height() as i32),
                file_size: Some(file_size),
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

            if let Err(e) = crate::db::insert_media(&app, &media) {
                let _ = fs::remove_file(&dest);
                results.push(crate::media::MediaImportResult {
                    id: String::new(),
                    path: img.path.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
                continue;
            }

            // Generate thumbnails (std::thread — no tokio runtime on main thread)
            let img_clone = decoded.clone();
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
            if !task.prompt.is_empty() {
                if let Err(e) = crate::db::caption_create_with_source(
                    &app, &id, &task.prompt, Some("ai-generated"),
                ) {
                    eprintln!("[image-queue] failed to save prompt caption: {}", e);
                }
            }

            // Trigger AI annotation (std::thread — no tokio runtime on main thread)
            let app_clone = app.clone();
            let mid = id.clone();
            let dest_clone = dest.clone();
            std::thread::spawn(move || {
                let queue = app_clone.state::<crate::ai::AiQueue>();
                let _ = queue.send(crate::ai::AiTask::GenerateCaption {
                    media_id: mid,
                    image_path: dest_clone,
                });
            });

            results.push(crate::media::MediaImportResult {
                id,
                path: img.path.clone(),
                success: true,
                error: None,
            });
        }
        // Remove imported staged images from task
        let selected_set: std::collections::HashSet<_> = selected_ids.iter().collect();
        task.staged.retain(|s| !selected_set.contains(&s.id));
        // If all staged images imported, remove the task entirely
        if task.staged.is_empty() {
            drop(tasks); // release lock before mutation in remove
            queue.tasks.lock().unwrap().remove(&task_id);
        }
        Ok(results)
    }
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

/// 取消正在运行的 ComfyUI 任务：通过已登记的 prompt_id 定向中断。
#[tauri::command]
pub async fn image_queue_cancel(app: AppHandle, task_id: String) -> Result<String, String> {
    let queue = app.state::<ImageQueue>();
    let prompt_id = queue
        .get_prompt_id(&task_id)
        .ok_or("任务不存在或尚未提交到 ComfyUI")?;
    {
        let tasks = queue.tasks.lock().unwrap();
        match tasks.get(&task_id) {
            Some(t) if t.status != "running" => return Err("任务未在运行".into()),
            Some(_) => {}
            None => return Err("任务不存在".into()),
        }
    }
    let base_url = crate::settings::get_comfyui_base_url(&app);
    let client = reqwest::Client::new();
    let url = format!("{}/interrupt", base_url);
    let resp = client
        .post(&url)
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
