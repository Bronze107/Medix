pub mod embedding;
pub mod imagine;
pub mod llamacpp;
pub mod server;

pub use embedding::EmbeddingServer;
pub use llamacpp::{embed_text, generate_caption, generate_caption_multi_image, resolve_prompt, SamplingParams};
pub use server::LlamaServer;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
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
                    AiTask::GenerateVideoCaption {
                        media_id,
                        video_path,
                        duration_secs,
                        variant_id,
                    } => {
                        if let Err(e) =
                            process_video_caption(app.clone(), media_id, video_path, duration_secs, variant_id).await
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
    let t_total = Instant::now();
    let ai_mode = crate::settings::get_ai_mode(&app);

    if ai_mode == "cloud" {
        eprintln!("[ai] cloud mode not yet implemented, skipping {}", media_id);
        return Ok(());
    }

    let port = crate::settings::get_llama_port(&app);

    // Check llama-server is running
    let t_health = Instant::now();
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
    let health_ms = t_health.elapsed().as_millis();

    let model = crate::settings::get_llama_model(&app);
    if model.is_empty() {
        eprintln!("[ai] no GGUF model configured, skipping {}", media_id);
        return Ok(());
    }

    println!("[ai] generating caption for {}...", media_id);

    // Resize image if max dim is configured
    // JPEG images use DCT-domain scaling (1/8 1/4 1/2) to avoid full 50MP decode.
    let t_resize = Instant::now();
    let inference_path: PathBuf;
    let inference_path_ref: &Path;
    let max_dim = crate::settings::get_llama_max_image_dim(&app);
    if max_dim > 0 {
        let mut magic = [0u8; 3];
        let is_jpeg = std::fs::File::open(&image_path)
            .ok()
            .and_then(|mut f| f.read_exact(&mut magic).ok())
            .is_some()
            && is_jpeg(&magic);

        let (img, w, h) = if is_jpeg {
            match decode_jpeg_fast(&image_path, max_dim) {
                Ok((dct_img, _)) => {
                    let (iw, ih) = (dct_img.width(), dct_img.height());
                    (dct_img, iw, ih)
                }
                Err(e) => {
                    eprintln!("[ai] DCT decode failed, falling back to full decode: {}", e);
                    let full = image::open(&image_path)
                        .map_err(|e| format!("failed to open image: {}", e))?;
                    let (fw, fh) = (full.width(), full.height());
                    (full, fw, fh)
                }
            }
        } else {
            let full = image::open(&image_path)
                .map_err(|e| format!("failed to open image: {}", e))?;
            let (fw, fh) = (full.width(), full.height());
            (full, fw, fh)
        };
        // Reset: DCT decode was already logged. Only time pixel-domain steps below.
        let t_pixel = Instant::now();

        let long_side = w.max(h);
        if long_side > max_dim {
            let ratio = max_dim as f64 / long_side as f64;
            let new_w = (w as f64 * ratio).round() as u32;
            let new_h = (h as f64 * ratio).round() as u32;
            let resized = img.resize_exact(
                new_w,
                new_h,
                image::imageops::FilterType::Nearest,
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
                "[ai] resized {}x{} → {}x{} for inference ({}ms)",
                w, h, new_w, new_h, t_pixel.elapsed().as_millis()
            );
        } else if is_jpeg {
            // DCT image already within max_dim, encode to temp file for VLM
            let mut buf = std::io::Cursor::new(Vec::new());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85)
                .encode_image(&img)
                .map_err(|e| format!("failed to encode image: {}", e))?;
            let tmp = std::env::temp_dir().join(format!("medix_infer_{}.jpg", media_id));
            tokio::fs::write(&tmp, buf.into_inner())
                .await
                .map_err(|e| format!("failed to write temp inference image: {}", e))?;
            inference_path = tmp;
            inference_path_ref = &inference_path;
        } else {
            inference_path_ref = &image_path;
        }
    } else {
        inference_path_ref = &image_path;
    }
    let resize_ms = t_resize.elapsed().as_millis();

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

    let t_infer = Instant::now();
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
    let infer_ms = t_infer.elapsed().as_millis();
    println!("[ai] inference took {}ms for {}", infer_ms, media_id);

    // Bilingual mode: call VLM twice — EN then ZH.
    if language == crate::settings::AiLanguage::Bilingual {
        let custom = custom_prompt.as_deref();
        let prompt_en = resolve_prompt(crate::settings::AiLanguage::English, custom);
        let prompt_zh = resolve_prompt(crate::settings::AiLanguage::Chinese, custom);

        // English call
        let t_en = Instant::now();
        let result_en = generate_caption(
            inference_path_ref, &model, port,
            Some(&prompt_en), None, &sampling,
        ).await.map_err(|e| {
            eprintln!("[ai] EN caption failed for {}: {}", media_id, e);
            e.to_string()
        })?;
        let en_ms = t_en.elapsed().as_millis();

        // Store EN caption
        let t_store_en = Instant::now();
        if let Some(ref variant_id) = variant_id {
            let _ = crate::db::caption_create_for_variant(&app, &media_id, variant_id, &result_en.caption, Some("ai_en"));
        } else {
            let _ = crate::db::caption_create_with_source(&app, &media_id, &result_en.caption, Some("ai_en"));
        }
        let store_en_ms = t_store_en.elapsed().as_millis();
        println!("[ai] EN caption stored for {}: {}... ({} tags) | EN infer={}ms store={}ms",
            media_id,
            result_en.caption.chars().take(40).collect::<String>(),
            result_en.tags.len(), en_ms, store_en_ms);

        // Chinese call
        let t_zh = Instant::now();
        let zh_infer_ms;
        match generate_caption(
            inference_path_ref, &model, port,
            Some(&prompt_zh), None, &sampling,
        ).await {
            Ok(result_zh) => {
                zh_infer_ms = t_zh.elapsed().as_millis();
                let t_store_zh = Instant::now();
                if let Some(ref variant_id) = variant_id {
                    let _ = crate::db::caption_create_for_variant(&app, &media_id, variant_id, &result_zh.caption, Some("ai_zh"));
                } else {
                    let _ = crate::db::caption_create_with_source(&app, &media_id, &result_zh.caption, Some("ai_zh"));
                }
                let store_zh_ms = t_store_zh.elapsed().as_millis();
                println!("[ai] ZH caption stored for {}: {}... | ZH infer={}ms store={}ms",
                    media_id,
                    result_zh.caption.chars().take(40).collect::<String>(),
                    zh_infer_ms, store_zh_ms);
            }
            Err(e) => {
                zh_infer_ms = t_zh.elapsed().as_millis();
                eprintln!("[ai] ZH caption failed for {}: {} | failed after {}ms", media_id, e, zh_infer_ms);
            }
        }

        // Store tags from English result
        let t_tags = Instant::now();
        let mut all_tags = crate::db::tag_list(&app).map_err(|e| e.to_string())?;
        for tag_name in &result_en.tags {
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
        let tags_ms = t_tags.elapsed().as_millis();

        // Generate embedding for EN caption
        let t_emb = Instant::now();
        if !result_en.caption.is_empty() {
            generate_caption_embedding(&app, &media_id, &result_en.caption, variant_id.as_deref()).await;
        }
        let emb_ms = t_emb.elapsed().as_millis();

        println!(
            "[ai] {} done in {}ms | health={}ms resize={}ms infer_en={}ms infer_zh={}ms tags={}ms emb={}ms",
            media_id, t_total.elapsed().as_millis(),
            health_ms, resize_ms, en_ms, zh_infer_ms, tags_ms, emb_ms
        );

        return Ok(());
    }

    println!(
        "[ai] caption generated for {}: {} ({} tags)",
        media_id,
        result.caption.chars().take(60).collect::<String>(),
        result.tags.len()
    );

    // Store caption with source='ai'
    let t_store = Instant::now();
    if let Some(ref variant_id) = variant_id {
        crate::db::caption_create_for_variant(&app, &media_id, variant_id, &result.caption, Some("ai"))
            .map_err(|e| e.to_string())?;
    } else {
        crate::db::caption_create_with_source(&app, &media_id, &result.caption, Some("ai"))
            .map_err(|e| e.to_string())?;
    }
    let store_ms = t_store.elapsed().as_millis();

    // Tags
    let t_tags = Instant::now();
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
    let tags_ms = t_tags.elapsed().as_millis();

    // Embedding
    let t_emb = Instant::now();
    if !result.caption.is_empty() {
        generate_caption_embedding(&app, &media_id, &result.caption, variant_id.as_deref()).await;
    }
    let emb_ms = t_emb.elapsed().as_millis();

    println!("[ai] {} done in {}ms | health={}ms resize={}ms infer={}ms store={}ms tags={}ms emb={}ms",
        media_id, t_total.elapsed().as_millis(),
        health_ms, resize_ms, infer_ms, store_ms, tags_ms, emb_ms);

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
    variant_id: Option<String>,
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
    let is_bilingual = language == crate::settings::AiLanguage::Bilingual;
    let mut zh_caption: Option<String> = None;

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

        // First call: use the resolved language prompt (English/Chinese).
        // For bilingual: call EN first, then ZH as a second call below.
        let first_prompt = if is_bilingual {
            resolve_prompt(crate::settings::AiLanguage::English, None)
        } else {
            system_prompt.clone()
        };
        // Language-aware user instruction (matches system prompt language)
        let user_instr: Option<&str> = if language == crate::settings::AiLanguage::Chinese {
            Some("这些帧来自同一段视频，按时间顺序排列。请综合所有帧进行分析，给出一个整体描述（不要逐帧分别描述）。涵盖视频的整体内容、场景、主体、光线、色彩、构图，以及帧与帧之间的变化、运动或进展。最后以一行 TAGS: 列出最显著的标签。")
        } else {
            None // default English
        };

        match crate::ai::llamacpp::generate_caption_multi_image(
            &path_refs,
            &model,
            port,
            Some(&first_prompt),
            user_instr,
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

        // Bilingual: second call with Chinese prompt (updates outer zh_caption)
        if is_bilingual {
            let zh_prompt = resolve_prompt(crate::settings::AiLanguage::Chinese, custom_prompt.as_deref());
            match crate::ai::llamacpp::generate_caption_multi_image(
                &path_refs, &model, port,
                Some(&zh_prompt), Some("这些帧来自同一段视频，按时间顺序排列。请综合所有帧进行分析，给出一个整体描述（不要逐帧分别描述）。涵盖视频的整体内容、场景、主体、光线、色彩、构图，以及帧与帧之间的变化、运动或进展。最后以一行 TAGS: 列出最显著的标签。"),
                &sampling,
            ).await {
                Ok(result) => {
                    let preview: String = result.caption.chars().take(40).collect();
                    println!("[video_ai] ZH caption generated: {}...", preview);
                    zh_caption = Some(result.caption);
                }
                Err(e) => eprintln!("[video_ai] ZH multi-frame inference failed: {}", e),
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
                if language == crate::settings::AiLanguage::Chinese {
                    Some(format!(
                        "视频的第 {}/{} 帧。请描述这一帧的画面内容，注意它是视频序列的一部分。",
                        i + 1, n
                    ))
                } else {
                    Some(format!(
                        "Frame {}/{} from a video. Describe what you see in this frame, \
                         keeping in mind it is part of a sequence.",
                        i + 1, n
                    ))
                }
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
    if is_bilingual && is_multi {
        // EN caption from the first multi-image call
        store_video_caption(&app, &media_id, variant_id.as_deref(), &merged_caption, "ai_en");
        // ZH caption from the second multi-image call
        if let Some(ref zh) = zh_caption {
            store_video_caption(&app, &media_id, variant_id.as_deref(), zh, "ai_zh");
        }
    } else if is_bilingual {
        // Single-frame bilingual: fall back to single "ai" caption
        store_video_caption(&app, &media_id, variant_id.as_deref(), &merged_caption, "ai");
    } else {
        // Single-language or single-frame: store as one caption
        store_video_caption(&app, &media_id, variant_id.as_deref(), &merged_caption, "ai");
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
            if let Some(ref vid) = variant_id {
                if let Err(e) = crate::db::media_tag_add_for_variant(
                    &app, &media_id, vid, &tag_id, Some("ai"),
                ) {
                    eprintln!("[video_ai] failed to add variant tag '{}': {}", tag_name, e);
                }
            } else if let Err(e) = crate::db::media_tag_add_with_source(
                &app, &media_id, &tag_id, Some(0.9), Some("ai"),
            ) {
                eprintln!("[video_ai] failed to add tag '{}': {}", tag_name, e);
            }
        }
    }

    // 10. Generate embedding
    if !merged_caption.is_empty() {
        generate_caption_embedding(&app, &media_id, &merged_caption, variant_id.as_deref()).await;
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
                img.resize_exact(new_w, new_h, image::imageops::FilterType::Nearest);
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

/// Store a caption for either a variant or the original media.
fn store_video_caption(
    app: &AppHandle,
    media_id: &str,
    variant_id: Option<&str>,
    caption: &str,
    source: &str,
) {
    let result = if let Some(vid) = variant_id {
        crate::db::caption_create_for_variant(app, media_id, vid, caption, Some(source))
            .map_err(|e| e.to_string())
    } else {
        crate::db::caption_create_with_source(app, media_id, caption, Some(source))
            .map_err(|e| e.to_string())
    };
    match result {
        Ok(_) => {
            let preview: String = caption.chars().take(40).collect();
            println!("[video_ai] {} caption stored: {}...", source, preview);
        }
        Err(e) => eprintln!("[video_ai] failed to store caption: {}", e),
    }
}

/// Generate and store a caption embedding via the dedicated embedding server.
async fn generate_caption_embedding(app: &AppHandle, media_id: &str, caption: &str, variant_id: Option<&str>) {
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
                crate::db::embedding_insert(app, media_id, &emb_model_short, "caption", variant_id, &vector)
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

/// Detect JPEG by magic bytes (FF D8 FF).
fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF]
}

/// Decode JPEG at reduced resolution using DCT-domain scaling, avoiding full decode.
/// Uses 1/8 (DC-only), 1/4, or 1/2 scaling to get closest to `max_dim` without going under.
/// Returns the decoded DynamicImage and elapsed ms.
fn decode_jpeg_fast(path: &Path, max_dim: u32) -> Result<(image::DynamicImage, u128), String> {
    let t = Instant::now();
    let jpeg_data = std::fs::read(path).map_err(|e| format!("read: {}", e))?;

    let mut decoder = libjpeg_turbo_rs::Decoder::new(&jpeg_data)
        .map_err(|e| format!("jpeg decoder: {}", e))?;
    let (hdr_w, hdr_h) = {
        let header = decoder.header();
        (header.width, header.height)
    };
    let long_side = (hdr_w.max(hdr_h)) as u32;

    let scale: u32 = if long_side / 8 >= max_dim { 8 }
        else if long_side / 4 >= max_dim { 4 }
        else if long_side / 2 >= max_dim { 2 }
        else { 1 };

    decoder.set_scale(libjpeg_turbo_rs::ScalingFactor::new(1, scale));
    let img = decoder.decode_image()
        .map_err(|e| format!("jpeg decode: {}", e))?;

    println!(
        "[ai] DCT decode 1/{}: {}x{} → {}x{} ({}ms)",
        scale,
        hdr_w, hdr_h,
        img.width, img.height,
        t.elapsed().as_millis()
    );

    let dynamic = image::DynamicImage::ImageRgb8(
        image::RgbImage::from_raw(img.width as u32, img.height as u32, img.data)
            .ok_or("failed to construct image from decoded data")?
    );
    Ok((dynamic, t.elapsed().as_millis()))
}
