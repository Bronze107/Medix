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

use super::{create_provider, EditParams, GenerateParams, ImageProvider, StagedImage};

const MAX_CONCURRENT: usize = 2;

// --- Task types ---

pub enum ImageTask {
    Generate {
        task_id: String,
        prompt: String,
        aspect_ratio: String,
        resolution: String,
        n: u32,
    },
    Edit {
        task_id: String,
        media_id: String,
        variant_id: Option<String>,
        prompt: String,
        aspect_ratio: String,
        resolution: String,
        n: u32,
    },
}

#[derive(Clone, Serialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub task_type: String,
    pub prompt: String,
    pub media_id: Option<String>,
    pub status: String,
    pub staged: Vec<StagedImage>,
    pub error: Option<String>,
    pub created_at: String,
}

struct TaskState {
    task_id: String,
    task_type: String,
    prompt: String,
    media_id: Option<String>,
    status: String,
    staged: Vec<StagedImage>,
    error: Option<String>,
    created_at: String,
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
    let (task_id, task_type, prompt, media_id, generate_params, edit_params) = match task {
        ImageTask::Generate {
            task_id,
            prompt,
            aspect_ratio,
            resolution,
            n,
        } => {
            let params = GenerateParams {
                prompt: prompt.clone(),
                aspect_ratio,
                resolution,
                n,
            };
            (task_id, "generate", prompt, None, Some(params), None)
        }
        ImageTask::Edit {
            task_id,
            media_id,
            variant_id,
            prompt,
            aspect_ratio,
            resolution,
            n,
        } => {
            // Resolve the image path
            let image_data_url = match resolve_edit_image(&app, &media_id, variant_id.as_deref(), &resolution)
            {
                Ok(url) => url,
                Err(e) => {
                    if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
                        t.status = "failed".to_string();
                        t.error = Some(e);
                    }
                    return;
                }
            };
            let params = EditParams {
                prompt: prompt.clone(),
                image_data_url,
                aspect_ratio,
                resolution,
                n,
            };
            (
                task_id,
                "edit",
                prompt,
                Some(media_id),
                None,
                Some(params),
            )
        }
    };

    // Update status to running
    if let Some(t) = tasks.lock().unwrap().get_mut(&task_id) {
        t.status = "running".to_string();
    }

    let provider = match create_provider(&app) {
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

/// Resolve the image path for editing and encode as data URL (reused from commands/imagine.rs).
fn resolve_edit_image(
    app: &AppHandle,
    media_id: &str,
    variant_id: Option<&str>,
    resolution: &str,
) -> Result<String, String> {
    let source_path = if let Some(vid) = variant_id {
        let variant = crate::db::variant_get_by_id(app, vid)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Variant not found".to_string())?;
        PathBuf::from(&variant.file_path)
    } else {
        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let library_dir = app_dir.join("library");
        let mut found = None;
        for entry in fs::read_dir(&library_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(&format!("{}.", media_id)) {
                found = Some(entry.path());
                break;
            }
        }
        found.ok_or("Original file not found".to_string())?
    };

    let img = image::open(&source_path).map_err(|e| e.to_string())?;
    let max_dim: u32 = match resolution {
        "2k" => 2048,
        _ => 1024,
    };
    let (w, h) = (img.width(), img.height());
    let image_data_url = if w.max(h) > max_dim {
        let ratio = max_dim as f64 / w.max(h) as f64;
        let new_w = (w as f64 * ratio).round() as u32;
        let new_h = (h as f64 * ratio).round() as u32;
        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        image_to_data_url(&resized, &source_path)?
    } else {
        image_to_data_url(&img, &source_path)?
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

fn image_to_data_url(img: &image::DynamicImage, source_path: &PathBuf) -> Result<String, String> {
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
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    n: Option<u32>,
) -> Result<String, String> {
    let queue = app.state::<ImageQueue>();
    let task_id = ulid::Ulid::new().to_string();
    let task = ImageTask::Generate {
        task_id: task_id.clone(),
        prompt: prompt.trim().to_string(),
        aspect_ratio: aspect_ratio.unwrap_or_else(|| "auto".to_string()),
        resolution: resolution.unwrap_or_else(|| "1k".to_string()),
        n: n.unwrap_or(1),
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
    });
    queue.send(task).map_err(|e| e.to_string())?;
    Ok(task_id)
}

#[tauri::command]
pub fn image_queue_submit_edit(
    app: AppHandle,
    media_id: String,
    variant_id: Option<String>,
    prompt: String,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    n: Option<u32>,
) -> Result<String, String> {
    let queue = app.state::<ImageQueue>();
    let task_id = ulid::Ulid::new().to_string();
    let task = ImageTask::Edit {
        task_id: task_id.clone(),
        media_id: media_id.clone(),
        variant_id: variant_id.clone(),
        prompt: prompt.trim().to_string(),
        aspect_ratio: aspect_ratio.unwrap_or_else(|| "auto".to_string()),
        resolution: resolution.unwrap_or_else(|| "1k".to_string()),
        n: n.unwrap_or(1),
    };
    queue.insert_task(TaskState {
        task_id: task_id.clone(),
        task_type: "edit".to_string(),
        prompt: prompt.trim().to_string(),
        media_id: Some(media_id),
        status: "pending".to_string(),
        staged: Vec::new(),
        error: None,
        created_at: Utc::now().to_rfc3339(),
    });
    queue.send(task).map_err(|e| e.to_string())?;
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
    let source = if task.media_id.is_some() {
        format!("edited:{}", provider)
    } else {
        format!("generated:{}", provider)
    };

    if let Some(ref mid) = task.media_id {
        // Edit mode: import as variants
        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let variants_dir = app_dir.join("variants");
        fs::create_dir_all(&variants_dir).map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for img in &selected {
            let ext = find_staged_ext(&staging_dir, &img.id)?;
            let src = staging_dir.join(format!("{}.{}", img.id, ext));
            let variant_id = ulid::Ulid::new().to_string();
            let dest = variants_dir.join(format!("{}_{}.{}", mid, variant_id, ext));
            fs::copy(&src, &dest).map_err(|e| e.to_string())?;
            let _ = fs::remove_file(&src);

            let decoded = image::open(&dest).map_err(|e| e.to_string())?;
            let file_size = fs::metadata(&dest).map_err(|e| e.to_string())?.len() as i64;

            let label = if task.prompt.len() > 50 {
                task.prompt[..50].to_string()
            } else {
                task.prompt.clone()
            };
            let variant = crate::variants::Variant {
                id: variant_id.clone(),
                media_id: mid.clone(),
                preset_name: String::new(),
                format: ext.clone(),
                width: Some(decoded.width() as i32),
                height: Some(decoded.height() as i32),
                quality: None,
                file_size: Some(file_size),
                file_path: dest.to_string_lossy().replace('\\', "/"),
                label: Some(label),
                source: Some(source.clone()),
            };
            crate::db::variant_insert(&app, &variant).map_err(|e| e.to_string())?;

            if let Err(e) = crate::db::caption_create_for_variant(
                &app, mid, &variant_id, &task.prompt, Some("ai-edit"),
            ) {
                eprintln!("[image-queue] failed to save prompt caption: {}", e);
            }

            results.push(crate::media::MediaImportResult {
                id: variant_id,
                path: dest.to_string_lossy().replace('\\', "/"),
                success: true,
                error: None,
            });
        }
        // Remove imported staged images from task
        let selected_set: std::collections::HashSet<_> = selected_ids.iter().collect();
        task.staged.retain(|s| !selected_set.contains(&s.id));
        if task.staged.is_empty() {
            task.status = "imported".to_string();
        }
        Ok(results)
    } else {
        // Generate mode: import as new media
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

            // Generate thumbnails
            let img_clone = decoded.clone();
            let app_clone = app.clone();
            let mid = id.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = crate::media::thumbnail::generate_thumbnails_from_image(
                    &app_clone, &mid, &img_clone,
                ) {
                    eprintln!("[image-queue] thumbnail failed: {}", e);
                }
            });

            // Save prompt as caption
            if let Err(e) =
                crate::db::caption_create_with_source(&app, &id, &task.prompt, Some("ai-generated"))
            {
                eprintln!("[image-queue] failed to save prompt caption: {}", e);
            }

            // Trigger AI annotation
            let app_clone = app.clone();
            let mid = id.clone();
            let dest_clone = dest.clone();
            tokio::task::spawn_blocking(move || {
                let queue = app_clone.state::<crate::ai::AiQueue>();
                let _ = queue.send(crate::ai::AiTask::GenerateCaption {
                    media_id: mid,
                    image_path: dest_clone,
                    variant_id: None,
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
        if task.staged.is_empty() {
            task.status = "imported".to_string();
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

fn find_staged_ext(staging: &std::path::Path, id: &str) -> Result<String, String> {
    for ext in &["jpg", "jpeg", "png", "webp"] {
        let path = staging.join(format!("{}.{}", id, ext));
        if path.exists() {
            return Ok(ext.to_string());
        }
    }
    Err(format!("Staged file not found for {}", id))
}
