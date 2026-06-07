pub mod embedding;
pub mod imagine;
pub mod llamacpp;
pub mod server;

pub use embedding::EmbeddingServer;
pub use llamacpp::{embed_text, generate_caption, generate_caption_multi_image, parse_bilingual_response, resolve_prompt, AiResult, BilingualResult, SamplingParams};
pub use server::LlamaServer;

use std::path::{Path, PathBuf};
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
    GenerateVideoCaption {
        media_id: String,
        video_path: PathBuf,
        duration_secs: f64,
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
                    AiTask::GenerateVideoCaption {
                        media_id,
                        video_path,
                        duration_secs,
                    } => {
                        if let Err(e) =
                            process_video_caption(app.clone(), media_id, video_path, duration_secs).await
                        {
                            eprintln!("[ai] failed to process video caption: {}", e);
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
    let language = crate::settings::get_ai_language(&app);
    let system_prompt = resolve_prompt(language, custom_prompt.as_deref());
    let sampling = SamplingParams {
        temperature: crate::settings::get_llama_temperature(&app),
        top_p: crate::settings::get_llama_top_p(&app),
        min_p: crate::settings::get_llama_min_p(&app),
        repeat_penalty: crate::settings::get_llama_repeat_penalty(&app),
        max_tokens: crate::settings::get_llama_max_tokens(&app),
        seed: crate::settings::get_llama_seed(&app),
    };
    let result = generate_caption(
        inference_path_ref,
        &model,
        port,
        Some(&system_prompt),
        None,
        &sampling,
    ).await.map_err(|e| {
        eprintln!("[ai] caption generation failed for {}: {}", media_id, e);
        e.to_string()
    })?;

    // Store caption(s). Bilingual mode stores EN + ZH as separate captions.
    if language == crate::settings::AiLanguage::Bilingual {
        let bilingual = parse_bilingual_response(&result.caption);
        // Store English caption
        if let Some(ref caption_en) = bilingual.caption_en {
            if let Some(ref variant_id) = variant_id {
                let _ = crate::db::caption_create_for_variant(&app, &media_id, variant_id, caption_en, Some("ai_en"));
            } else {
                let _ = crate::db::caption_create_with_source(&app, &media_id, caption_en, Some("ai_en"));
            }
            println!("[ai] EN caption stored for {}: {}...", media_id, caption_en.chars().take(40).collect::<String>());
        }
        // Store Chinese caption
        if let Some(ref caption_zh) = bilingual.caption_zh {
            if let Some(ref variant_id) = variant_id {
                let _ = crate::db::caption_create_for_variant(&app, &media_id, variant_id, caption_zh, Some("ai_zh"));
            } else {
                let _ = crate::db::caption_create_with_source(&app, &media_id, caption_zh, Some("ai_zh"));
            }
            println!("[ai] ZH caption stored for {}: {}...", media_id, caption_zh.chars().take(40).collect::<String>());
        }
        // Use bilingual tags (English danbooru-style) for tagging
        let tags_to_store = &bilingual.tags;
        println!(
            "[ai] bilingual caption generated for {}: {} tags",
            media_id,
            tags_to_store.len()
        );
        // Store tags
        let mut all_tags = crate::db::tag_list(&app).map_err(|e| e.to_string())?;
        for tag_name in tags_to_store {
            let tag_id = match ensure_tag_cached(&app, &mut all_tags, tag_name) {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("[ai] failed to ensure tag '{}': {}", tag_name, e);
                    continue;
                }
            };
            if let Some(ref vid) = variant_id {
                if let Err(e) = crate::db::media_tag_add_for_variant(&app, &media_id, vid, &tag_id, Some("ai")) {
                    eprintln!("[ai] failed to add variant tag '{}': {}", tag_name, e);
                }
            } else if let Err(e) =
                crate::db::media_tag_add_with_source(&app, &media_id, &tag_id, Some(0.9), Some("ai"))
            {
                eprintln!("[ai] failed to add tag '{}': {}", tag_name, e);
            }
        }
        // Generate embedding for English caption only
        if variant_id.is_none() {
            if let Some(ref caption_en) = bilingual.caption_en {
                generate_caption_embedding(&app, &media_id, caption_en).await;
            }
        }
        // Early return — bilingual path is self-contained
        return Ok(());
    }

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

    // Load all existing tags once, then do in-memory lookups per tag.
    // Saves ~10-15 `SELECT * FROM tags` round trips per image.
    let mut all_tags = crate::db::tag_list(&app).map_err(|e| e.to_string())?;
    for tag_name in &result.tags {
        let tag_id = match ensure_tag_cached(&app, &mut all_tags, tag_name) {
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

    // Generate caption embedding via dedicated embedding server (only for original, not variants).
    if variant_id.is_none() && !result.caption.is_empty() {
        let emb_model = crate::settings::get_embedding_model(&app);
        if emb_model.is_empty() {
            eprintln!("[ai] no embedding model configured, skipping embedding for {}", media_id);
        } else {
            let emb_port = crate::settings::get_embedding_port(&app);
            let emb_server = app.state::<EmbeddingServer>();
            if emb_server.health_check(emb_port).await {
                let emb_model_short = std::path::Path::new(&emb_model)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&emb_model)
                    .to_string();
                match embed_text(&result.caption, &emb_model, emb_port).await {
                    Ok(vector) => {
                        if let Err(e) =
                            crate::db::embedding_insert(&app, &media_id, &emb_model_short, "caption", &vector)
                        {
                            eprintln!("[ai] failed to store caption embedding: {}", e);
                        } else {
                            println!("[ai] embedding stored for {} ({}d)", media_id, vector.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("[ai] embedding failed: {}", e);
                    }
                }
            } else {
                eprintln!("[ai] embedding server not running, skipping embedding for {}", media_id);
            }
        }
    }

    println!("[ai] completed processing {}", media_id);
    // Clean up temp inference file (if any)
    if inference_path_ref != &image_path {
        let _ = tokio::fs::remove_file(inference_path_ref).await;
    }
    Ok(())
}

fn ensure_tag_cached(
    app: &AppHandle,
    existing: &mut Vec<crate::tag::Tag>,
    name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let name_lower = name.to_lowercase();
    if let Some(tag) = existing.iter().find(|t| t.name == name_lower) {
        return Ok(tag.id.clone());
    }
    let id = crate::db::tag_create(app, &name_lower)?;
    // Push to local cache so subsequent lookups in this batch find it
    existing.push(crate::tag::Tag {
        id: id.clone(),
        name: name_lower,
        source: Some("ai".to_string()),
        confidence: None,
        item_count: Some(1i64),
    });
    Ok(id)
}

async fn process_video_caption(
    app: AppHandle,
    media_id: String,
    video_path: PathBuf,
    duration_secs: f64,
) -> Result<(), String> {
    // 1. Check AI mode
    let ai_mode = crate::settings::get_ai_mode(&app);
    if ai_mode == "cloud" {
        println!("[video_ai] cloud mode not supported for video, skipping {}", media_id);
        return Ok(());
    }

    // 2. Check llama-server health
    let port = crate::settings::get_llama_port(&app);
    let server = app.state::<LlamaServer>();
    if !server.health_check(port).await {
        println!("[video_ai] llama-server not running, skipping {}", media_id);
        return Ok(());
    }

    // 3. Check model configured
    let model = crate::settings::get_llama_model(&app);
    if model.is_empty() {
        println!("[video_ai] no VLM model configured, skipping {}", media_id);
        return Ok(());
    }

    // 4. Get settings
    let n_frames = crate::settings::get_video_ai_frame_count(&app);
    let custom_prompt = crate::settings::get_ai_custom_prompt(&app);
    let language = crate::settings::get_ai_language(&app);
    let system_prompt = resolve_prompt(language, custom_prompt.as_deref());
    let sampling = SamplingParams {
        temperature: crate::settings::get_llama_temperature(&app),
        top_p: crate::settings::get_llama_top_p(&app),
        min_p: crate::settings::get_llama_min_p(&app),
        repeat_penalty: crate::settings::get_llama_repeat_penalty(&app),
        max_tokens: crate::settings::get_llama_max_tokens(&app),
        seed: crate::settings::get_llama_seed(&app),
    };

    // 5. Extract frames
    println!("[video_ai] extracting {} frames from {}", n_frames, video_path.display());
    let frames = match crate::media::video_metadata::extract_frames(
        &video_path,
        duration_secs,
        n_frames,
    ) {
        Ok(f) if !f.is_empty() => f,
        Ok(_) => {
            println!("[video_ai] no frames extracted, skipping");
            return Ok(());
        }
        Err(e) => {
            eprintln!("[video_ai] frame extraction failed: {}", e);
            return Err(e);
        }
    };

    // 6. Send frames to VLM (single-frame loop or multi-frame batch)
    let mut all_captions: Vec<String> = Vec::new();
    let mut all_tags: Vec<String> = Vec::new();
    let n = frames.len();

    let multi_frame = crate::settings::is_video_ai_multi_frame(&app);

    if multi_frame && n > 1 {
        // --- Multi-frame path: resize all frames, then send in one request ---
        let mut inference_paths: Vec<PathBuf> = Vec::with_capacity(n);
        let mut tmp_paths: Vec<PathBuf> = Vec::new(); // track temp files for cleanup

        let max_dim = crate::settings::get_llama_max_image_dim(&app);
        for (i, frame_path) in frames.iter().enumerate() {
            let inf_path = resize_frame_for_inference(
                frame_path, &media_id, i + 1, n, max_dim,
            )
            .await;
            if inf_path != *frame_path {
                tmp_paths.push(inf_path.clone());
            }
            inference_paths.push(inf_path);
        }

        let path_refs: Vec<&Path> = inference_paths.iter().map(|p| p.as_path()).collect();
        println!(
            "[video_ai] multi-frame mode: sending {} frames in one request",
            path_refs.len()
        );

        match crate::ai::llamacpp::generate_caption_multi_image(
            &path_refs,
            &model,
            port,
            Some(&system_prompt),
            &sampling,
        )
        .await
        {
            Ok(result) => {
                all_captions.push(result.caption);
                all_tags = result.tags;
            }
            Err(e) => {
                eprintln!("[video_ai] multi-frame inference failed: {}", e);
            }
        }

        // Clean up temp resized frames
        for tmp in &tmp_paths {
            let _ = std::fs::remove_file(tmp);
        }
    } else {
        // --- Single-frame path (original loop) ---
        for (i, frame_path) in frames.iter().enumerate() {
            let frame_user_text = if n > 1 {
                Some(format!(
                    "Frame {}/{} from a video. Describe what you see in this frame, \
                     keeping in mind it is part of a sequence.",
                    i + 1, n
                ))
            } else {
                None
            };

            let inference_path_ref = {
                let max_dim = crate::settings::get_llama_max_image_dim(&app);
                resize_frame_for_inference(
                    frame_path, &media_id, i + 1, n, max_dim,
                )
                .await
            };
            let is_tmp = inference_path_ref != *frame_path;

            match crate::ai::llamacpp::generate_caption(
                &inference_path_ref,
                &model,
                port,
                Some(&system_prompt),
                frame_user_text.as_deref(),
                &sampling,
            )
            .await
            {
                Ok(result) => {
                    all_captions.push(result.caption);
                    all_tags.extend(result.tags);
                }
                Err(e) => {
                    eprintln!("[video_ai] frame {}/{} inference failed: {}", i + 1, n, e);
                }
            }

            // Clean up temp resized frame if one was created
            if is_tmp {
                let _ = std::fs::remove_file(&inference_path_ref);
            }
        }
    }

    // 7. Merge results (single-frame path may have multiple captions)
    if all_captions.is_empty() {
        println!("[video_ai] no captions generated from any frame, skipping");
        crate::media::video_metadata::cleanup_frames(&frames);
        return Ok(());
    }

    let is_multi = multi_frame && n > 1;
    let merged_caption = if is_multi {
        all_captions.into_iter().next().unwrap_or_default()
    } else {
        dedup_join(&all_captions, " | ")
    };
    all_tags.sort();
    all_tags.dedup();

    // 8. Store caption(s). Bilingual multi-frame stores EN + ZH separately.
    let is_bilingual = language == crate::settings::AiLanguage::Bilingual;
    if is_bilingual && is_multi {
        let bilingual = parse_bilingual_response(&merged_caption);
        if let Some(ref caption_en) = bilingual.caption_en {
            let _ = crate::db::caption_create_with_source(&app, &media_id, caption_en, Some("ai_en"));
            println!("[video_ai] EN caption stored: {}...", caption_en.chars().take(40).collect::<String>());
        }
        if let Some(ref caption_zh) = bilingual.caption_zh {
            let _ = crate::db::caption_create_with_source(&app, &media_id, caption_zh, Some("ai_zh"));
            println!("[video_ai] ZH caption stored: {}...", caption_zh.chars().take(40).collect::<String>());
        }
        // Use bilingual tags
        all_tags = bilingual.tags;
        all_tags.sort();
        all_tags.dedup();
    } else {
        // Single-language or single-frame: store as one caption
        match crate::db::caption_create_with_source(
            &app,
            &media_id,
            &merged_caption,
            Some("ai"),
        ) {
            Ok(cap) => {
                let preview: String = merged_caption.chars().take(60).collect();
                println!("[video_ai] caption stored: {} (id={})", preview, cap.id);
            }
            Err(e) => eprintln!("[video_ai] failed to store caption: {}", e),
        }
    }

    // 9. Store tags
    if !all_tags.is_empty() {
        let mut db_tags = crate::db::tag_list(&app).map_err(|e| e.to_string())?;
        for tag_name in &all_tags {
            let tag_id = match ensure_tag_cached(&app, &mut db_tags, tag_name) {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("[video_ai] failed to ensure tag '{}': {}", tag_name, e);
                    continue;
                }
            };
            if let Err(e) = crate::db::media_tag_add_with_source(
                &app, &media_id, &tag_id, Some(0.9), Some("ai"),
            ) {
                eprintln!("[video_ai] failed to add tag '{}': {}", tag_name, e);
            }
        }
    }

    // 10. Generate embedding (use EN caption for bilingual, merged for others)
    let emb_caption = if is_bilingual && is_multi {
        parse_bilingual_response(&merged_caption).caption_en.unwrap_or_default()
    } else {
        merged_caption.clone()
    };
    if !emb_caption.is_empty() {
        generate_caption_embedding(&app, &media_id, &emb_caption).await;
    }

    // 11. Cleanup temp frames
    crate::media::video_metadata::cleanup_frames(&frames);

    let preview: String = merged_caption.chars().take(80).collect();
    println!(
        "[video_ai] done — {} frames processed, caption: {}...",
        n, preview
    );
    Ok(())
}

/// Resize a frame to fit within max_dim, returning the path to use for inference.
/// If no resize is needed, returns the original path. Otherwise writes a temp JPEG
/// and returns the temp path. The caller is responsible for cleaning up temp files.
async fn resize_frame_for_inference(
    frame_path: &Path,
    media_id: &str,
    frame_idx: usize,
    total_frames: usize,
    max_dim: u32,
) -> PathBuf {
    if max_dim == 0 {
        return frame_path.to_path_buf();
    }
    match image::open(frame_path) {
        Ok(img) => {
            let (w, h) = (img.width(), img.height());
            let long_side = w.max(h);
            if long_side <= max_dim {
                return frame_path.to_path_buf();
            }
            let ratio = max_dim as f64 / long_side as f64;
            let new_w = (w as f64 * ratio).round() as u32;
            let new_h = (h as f64 * ratio).round() as u32;
            let resized =
                img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
            let mut buf = std::io::Cursor::new(Vec::new());
            if image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85)
                .encode_image(&resized)
                .is_ok()
            {
                let tmp = std::env::temp_dir().join(format!(
                    "medix_video_infer_{}_{}.jpg",
                    media_id, frame_idx
                ));
                if tokio::fs::write(&tmp, buf.into_inner()).await.is_ok() {
                    println!(
                        "[video_ai] frame {}/{} resized {}x{} → {}x{}",
                        frame_idx, total_frames, w, h, new_w, new_h
                    );
                    return tmp;
                }
            }
            frame_path.to_path_buf()
        }
        Err(e) => {
            eprintln!(
                "[video_ai] frame {}/{} cannot open for resize: {}",
                frame_idx, total_frames, e
            );
            frame_path.to_path_buf()
        }
    }
}

/// Join strings, removing consecutive duplicates.
fn dedup_join(items: &[String], separator: &str) -> String {
    let mut result = String::new();
    let mut prev: Option<&str> = None;
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if prev == Some(trimmed) {
            continue;
        }
        if !result.is_empty() {
            result.push_str(separator);
        }
        result.push_str(trimmed);
        prev = Some(trimmed);
    }
    result
}

/// Generate and store a caption embedding via the dedicated embedding server.
async fn generate_caption_embedding(app: &AppHandle, media_id: &str, caption: &str) {
    let emb_model = crate::settings::get_embedding_model(app);
    if emb_model.is_empty() {
        eprintln!("[ai] no embedding model configured, skipping embedding for {}", media_id);
        return;
    }
    let emb_port = crate::settings::get_embedding_port(app);
    let emb_server = app.state::<EmbeddingServer>();
    if !emb_server.health_check(emb_port).await {
        eprintln!("[ai] embedding server not running, skipping embedding for {}", media_id);
        return;
    }
    let emb_model_short = std::path::Path::new(&emb_model)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&emb_model)
        .to_string();
    match embed_text(caption, &emb_model, emb_port).await {
        Ok(vector) => {
            if let Err(e) =
                crate::db::embedding_insert(app, media_id, &emb_model_short, "caption", &vector)
            {
                eprintln!("[ai] failed to store caption embedding for {}: {}", media_id, e);
            } else {
                println!("[ai] embedding stored for {} ({}d)", media_id, vector.len());
            }
        }
        Err(e) => {
            eprintln!("[ai] embedding failed for {}: {}", media_id, e);
        }
    }
}
