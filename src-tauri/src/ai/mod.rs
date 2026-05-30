pub mod llamacpp;
pub mod server;

pub use llamacpp::{embed_text, generate_caption, AiResult, SamplingParams};
pub use server::LlamaServer;

use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{AppHandle, Emitter, Manager};

pub enum AiTask {
    GenerateCaption {
        media_id: String,
        image_path: PathBuf,
        variant_id: Option<String>,
    },
}

#[derive(Clone, serde::Serialize)]
pub struct AiTaskProgress {
    pub remaining: usize,
}

#[derive(Clone)]
pub struct AiQueue {
    sender: Sender<AiTask>,
    pending: Arc<AtomicUsize>,
}

impl AiQueue {
    pub fn send(&self, task: AiTask) -> Result<(), mpsc::SendError<AiTask>> {
        self.pending.fetch_add(1, Ordering::SeqCst);
        self.sender.send(task)
    }

    pub fn pending_count(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }
}

pub fn init_ai_queue(app: AppHandle) -> AiQueue {
    let (tx, rx) = mpsc::channel::<AiTask>();
    let pending = Arc::new(AtomicUsize::new(0));
    let pending_clone = pending.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for ai queue");
        rt.block_on(async move {
            while let Ok(task) = rx.recv() {
                match task {
                    AiTask::GenerateCaption {
                        media_id,
                        image_path,
                        variant_id,
                    } => {
                        if let Err(e) =
                            process_generate_caption(app.clone(), media_id, image_path, variant_id).await
                        {
                            eprintln!("[ai] failed to process caption generation: {}", e);
                        }
                        let remaining = pending_clone.fetch_sub(1, Ordering::SeqCst) - 1;
                        let _ = app.emit("ai-task-done", AiTaskProgress { remaining });
                    }
                }
            }
        });
    });

    AiQueue { sender: tx, pending }
}

async fn process_generate_caption(
    app: AppHandle,
    media_id: String,
    image_path: PathBuf,
    variant_id: Option<String>,
) -> Result<(), String> {
    let ai_mode = crate::settings::get_ai_mode(&app);

    if ai_mode == "cloud" {
        eprintln!("[ai] cloud mode not yet implemented, skipping {}", media_id);
        return Ok(());
    }

    let port = crate::settings::get_llama_port(&app);

    // Check llama-server is running
    let server = app.state::<LlamaServer>();
    if !server.health_check(port).await {
        if ai_mode == "local" {
            eprintln!(
                "[ai] llama-server not running and mode is local, skipping {}",
                media_id
            );
        } else {
            eprintln!(
                "[ai] llama-server not running (auto mode), skipping {}",
                media_id
            );
        }
        return Ok(());
    }

    let model = crate::settings::get_llama_model(&app);
    let model_short = std::path::Path::new(&model)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&model)
        .to_string();
    if model.is_empty() {
        eprintln!("[ai] no GGUF model configured, skipping {}", media_id);
        return Ok(());
    }

    println!("[ai] generating caption for {}...", media_id);

    // Resize image if max dim is configured
    let inference_path: PathBuf;
    let inference_path_ref: &PathBuf;
    let max_dim = crate::settings::get_llama_max_image_dim(&app);
    if max_dim > 0 {
        let img = image::open(&image_path)
            .map_err(|e| format!("failed to open image for resize: {}", e))?;
        let (w, h) = (img.width(), img.height());
        let long_side = w.max(h);
        if long_side > max_dim {
            let ratio = max_dim as f64 / long_side as f64;
            let new_w = (w as f64 * ratio).round() as u32;
            let new_h = (h as f64 * ratio).round() as u32;
            let resized = img.resize_exact(
                new_w,
                new_h,
                image::imageops::FilterType::Lanczos3,
            );
            let mut buf = std::io::Cursor::new(Vec::new());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85)
                .encode_image(&resized)
                .map_err(|e| format!("failed to encode resized image: {}", e))?;
            let tmp = std::env::temp_dir().join(format!("medix_infer_{}.jpg", media_id));
            tokio::fs::write(&tmp, buf.into_inner())
                .await
                .map_err(|e| format!("failed to write temp inference image: {}", e))?;
            inference_path = tmp;
            inference_path_ref = &inference_path;
            println!(
                "[ai] resized {}x{} → {}x{} for inference",
                w, h, new_w, new_h
            );
        } else {
            inference_path_ref = &image_path;
        }
    } else {
        inference_path_ref = &image_path;
    }

    let custom_prompt = crate::settings::get_ai_custom_prompt(&app);
    let sampling = SamplingParams {
        temperature: crate::settings::get_llama_temperature(&app),
        top_p: crate::settings::get_llama_top_p(&app),
        min_p: crate::settings::get_llama_min_p(&app),
        repeat_penalty: crate::settings::get_llama_repeat_penalty(&app),
        max_tokens: crate::settings::get_llama_max_tokens(&app),
    };
    let result = generate_caption(
        inference_path_ref,
        &model,
        port,
        custom_prompt.as_deref(),
        &sampling,
    ).await.map_err(|e| {
        eprintln!("[ai] caption generation failed for {}: {}", media_id, e);
        e.to_string()
    })?;

    println!(
        "[ai] caption generated for {}: {} ({} tags)",
        media_id,
        result.caption.chars().take(60).collect::<String>(),
        result.tags.len()
    );

    // Store caption with source='ai'
    if let Some(ref variant_id) = variant_id {
        crate::db::caption_create_for_variant(&app, &media_id, variant_id, &result.caption, Some("ai"))
            .map_err(|e| e.to_string())?;
    } else {
        crate::db::caption_create_with_source(&app, &media_id, &result.caption, Some("ai"))
            .map_err(|e| e.to_string())?;
    }

    // Store AI tags with source='ai' and confidence=0.9
    for tag_name in &result.tags {
        let tag_id = match ensure_tag(&app, tag_name) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("[ai] failed to ensure tag '{}': {}", tag_name, e);
                continue;
            }
        };
        if let Some(ref vid) = variant_id {
            if let Err(e) = crate::db::media_tag_add_for_variant(
                &app, &media_id, vid, &tag_id, Some("ai"),
            ) {
                eprintln!("[ai] failed to add variant tag '{}': {}", tag_name, e);
            }
        } else if let Err(e) =
            crate::db::media_tag_add_with_source(&app, &media_id, &tag_id, Some(0.9), Some("ai"))
        {
            eprintln!("[ai] failed to add tag '{}': {}", tag_name, e);
        }
    }

    // Generate embedding for caption (only for original, not variants)
    if variant_id.is_none() {
        match embed_text(&result.caption, &model, port).await {
            Ok(vector) => {
                if let Err(e) =
                    crate::db::embedding_insert(&app, &media_id, &model_short,"caption", &vector)
                {
                    eprintln!("[ai] failed to store caption embedding: {}", e);
                }
            }
            Err(e) => {
                eprintln!("[ai] caption embedding failed: {}", e);
            }
        }

        // Generate embedding for tags
        if !result.tags.is_empty() {
            let tags_text = result.tags.join(", ");
            match embed_text(&tags_text, &model, port).await {
                Ok(vector) => {
                    if let Err(e) =
                        crate::db::embedding_insert(&app, &media_id, &model_short,"tags", &vector)
                    {
                        eprintln!("[ai] failed to store tags embedding: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("[ai] tags embedding failed: {}", e);
                }
            }
        }
    }

    println!("[ai] completed processing {}", media_id);
    Ok(())
}

fn ensure_tag(app: &AppHandle, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let existing = crate::db::tag_list(app)?;
    let name_lower = name.to_lowercase();
    if let Some(tag) = existing.iter().find(|t| t.name == name_lower) {
        return Ok(tag.id.clone());
    }
    crate::db::tag_create(app, &name_lower)
}
