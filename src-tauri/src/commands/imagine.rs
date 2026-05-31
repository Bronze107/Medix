use base64::Engine;
use image::ImageEncoder;
use std::fs;
use std::path::Path;
use tauri::{command, AppHandle, Manager};
use ulid::Ulid;

use crate::ai::imagine::{self, EditParams, GenerateParams, StagedImage};
use crate::settings;

fn staging_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let staging = app_dir.join("staging");
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    Ok(staging)
}

fn resolve_media_path(app: &AppHandle, media_id: &str) -> Result<std::path::PathBuf, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library_dir = app_dir.join("library");
    for entry in fs::read_dir(&library_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(&format!("{}.", media_id)) {
            return Ok(entry.path());
        }
    }
    Err("Original file not found".to_string())
}

/// Generate images from a text prompt. Results are staged — call confirm_import to finalize.
#[command]
pub async fn image_generate(
    app: AppHandle,
    prompt: String,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    n: Option<u32>,
) -> Result<Vec<StagedImage>, String> {
    let provider = imagine::create_provider(&app).map_err(|e| e.to_string())?;

    let params = GenerateParams {
        prompt: prompt.clone(),
        aspect_ratio: aspect_ratio.unwrap_or_else(|| "auto".to_string()),
        resolution: resolution.unwrap_or_else(|| "1k".to_string()),
        n: n.unwrap_or(1),
    };

    let images = provider.generate(&params).await.map_err(|e| e.to_string())?;

    let staging = staging_dir(&app)?;
    let mut results = Vec::new();

    for img in &images {
        let id = Ulid::new().to_string();
        let ext = match img.mime_type.as_str() {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            _ => "png",
        };
        let temp_path = staging.join(format!("{}.{}", id, ext));
        fs::write(&temp_path, &img.data).map_err(|e| e.to_string())?;

        // Get dimensions
        let decoded = image::open(&temp_path).map_err(|e| e.to_string())?;
        let file_size = fs::metadata(&temp_path).map_err(|e| e.to_string())?.len() as i64;

        results.push(StagedImage {
            id,
            width: decoded.width() as i32,
            height: decoded.height() as i32,
            file_size,
        });
    }

    Ok(results)
}

/// Edit an existing image. Results are staged — call confirm_import to finalize.
#[command]
pub async fn image_edit(
    app: AppHandle,
    media_id: String,
    prompt: String,
    resolution: Option<String>,
    n: Option<u32>,
) -> Result<Vec<StagedImage>, String> {
    let source_path = resolve_media_path(&app, &media_id)?;
    let resolution = resolution.unwrap_or_else(|| "1k".to_string());

    // Preprocess: resize if needed, keep original format
    let img = image::open(&source_path).map_err(|e| e.to_string())?;
    let max_dim: u32 = match resolution.as_str() {
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

    // Check request body size
    let b64_len = image_data_url.len();
    const MAX_BODY: usize = 10 * 1024 * 1024; // 10MB
    if b64_len > MAX_BODY {
        return Err(format!(
            "Image too large after encoding ({}MB > 10MB limit). Try a lower resolution.",
            b64_len / (1024 * 1024)
        ));
    }

    let provider = imagine::create_provider(&app).map_err(|e| e.to_string())?;
    let params = EditParams {
        prompt: prompt.clone(),
        image_data_url,
        resolution,
        n: n.unwrap_or(1),
    };

    let images = provider.edit(&params).await.map_err(|e| e.to_string())?;

    let staging = staging_dir(&app)?;
    let mut results = Vec::new();

    for img in &images {
        let id = Ulid::new().to_string();
        let ext = match img.mime_type.as_str() {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            _ => "png",
        };
        let temp_path = staging.join(format!("{}.{}", id, ext));
        fs::write(&temp_path, &img.data).map_err(|e| e.to_string())?;

        let decoded = image::open(&temp_path).map_err(|e| e.to_string())?;
        let file_size = fs::metadata(&temp_path).map_err(|e| e.to_string())?.len() as i64;

        results.push(StagedImage {
            id,
            width: decoded.width() as i32,
            height: decoded.height() as i32,
            file_size,
        });
    }

    Ok(results)
}

/// Confirm import — move staged images into the library (generation) or variants (editing).
#[command]
pub async fn image_confirm_import(
    app: AppHandle,
    staged_ids: Vec<String>,
    prompt: String,
    media_id: Option<String>,
) -> Result<Vec<crate::media::MediaImportResult>, String> {
    if staged_ids.is_empty() {
        return Ok(Vec::new());
    }

    let staging = staging_dir(&app)?;
    let provider = settings::get_image_api_provider(&app);
    let source = if let Some(_media_id) = media_id.as_ref() {
        format!("edited:{}", provider)
    } else {
        format!("generated:{}", provider)
    };

    if let Some(ref mid) = media_id {
        // Editing mode: import as variants
        let variants_dir = {
            let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            app_dir.join("variants")
        };
        fs::create_dir_all(&variants_dir).map_err(|e| e.to_string())?;

        let mid_str: &str = mid.as_ref();
        let mut results = Vec::new();
        for sid in &staged_ids {
            let ext = find_staged_ext(&staging, sid)?;
            let src = staging.join(format!("{}.{}", sid, ext));
            let variant_id = Ulid::new().to_string();
            let dest = variants_dir.join(format!("{}_{}.{}", mid_str, variant_id, ext));
            fs::copy(&src, &dest).map_err(|e| e.to_string())?;
            let _ = fs::remove_file(&src);

            let img = image::open(&dest).map_err(|e| e.to_string())?;
            let file_size = fs::metadata(&dest).map_err(|e| e.to_string())?.len() as i64;

            let label = if prompt.len() > 50 { prompt[..50].to_string() } else { prompt.clone() };
            let variant = crate::variants::Variant {
                id: variant_id.clone(),
                media_id: mid_str.to_string(),
                preset_name: String::new(),
                format: ext.clone(),
                width: Some(img.width() as i32),
                height: Some(img.height() as i32),
                quality: None,
                file_size: Some(file_size),
                file_path: dest.to_string_lossy().replace('\\', "/"),
                label: Some(label),
                source: Some(source.clone()),
            };
            crate::db::variant_insert(&app, &variant).map_err(|e| e.to_string())?;

            // Store prompt as caption on the variant
            if let Err(e) = crate::db::caption_create_for_variant(
                &app, mid_str, &variant_id, &prompt, Some("ai-edit"),
            ) {
                eprintln!("[imagine] failed to save prompt caption: {}", e);
            }

            results.push(crate::media::MediaImportResult {
                id: variant_id,
                path: dest.to_string_lossy().replace('\\', "/"),
                success: true,
                error: None,
            });
        }
        Ok(results)
    } else {
        // Generation mode: import as new media items via the import pipeline
        let mut paths = Vec::new();
        for sid in &staged_ids {
            let ext = find_staged_ext(&staging, sid)?;
            paths.push(staging.join(format!("{}.{}", sid, ext)).to_string_lossy().to_string());
        }
        let library_dir = {
            let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            app_dir.join("library")
        };

        let mut results = Vec::new();
        for (i, path_str) in paths.iter().enumerate() {
            let src = Path::new(path_str);
            let id = if let Some(sid) = staged_ids.get(i) { sid.clone() } else { Ulid::new().to_string() };
            let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png");
            let dest = library_dir.join(format!("{}.{}", id, ext));

            if let Err(e) = fs::copy(src, &dest) {
                results.push(crate::media::MediaImportResult { id: String::new(), path: path_str.clone(), success: false, error: Some(e.to_string()) });
                continue;
            }
            let _ = fs::remove_file(src);

            let img = match image::open(&dest) {
                Ok(i) => i,
                Err(e) => {
                    results.push(crate::media::MediaImportResult { id: String::new(), path: path_str.clone(), success: false, error: Some(e.to_string()) });
                    continue;
                }
            };
            let file_size = fs::metadata(&dest).map(|m| m.len() as i64).unwrap_or(0);

            let media = crate::media::Media {
                id: id.clone(),
                source_path: None,
                width: Some(img.width() as i32),
                height: Some(img.height() as i32),
                file_size: Some(file_size),
                created_at: None,
                modified_at: None,
                imported_at: chrono::Utc::now().to_rfc3339(),
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
                results.push(crate::media::MediaImportResult { id: String::new(), path: path_str.clone(), success: false, error: Some(e.to_string()) });
                continue;
            }

            // Generate thumbnails
            let img_clone = img.clone();
            let app_clone = app.clone();
            let mid = id.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = crate::media::thumbnail::generate_thumbnails_from_image(&app_clone, &mid, &img_clone) {
                    eprintln!("[imagine] thumbnail failed: {}", e);
                }
            });

            // Save prompt as caption
            if let Err(e) = crate::db::caption_create_with_source(&app, &id, &prompt, Some("ai-generated")) {
                eprintln!("[imagine] failed to save prompt caption: {}", e);
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

            results.push(crate::media::MediaImportResult { id, path: path_str.clone(), success: true, error: None });
        }
        Ok(results)
    }
}

/// Discard staged images — delete temp files without importing.
#[command]
pub fn image_discard_staged(app: AppHandle, staged_ids: Vec<String>) -> Result<(), String> {
    let staging = staging_dir(&app)?;
    for sid in &staged_ids {
        if let Ok(ext) = find_staged_ext(&staging, sid) {
            let path = staging.join(format!("{}.{}", sid, ext));
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

fn find_staged_ext(staging: &Path, id: &str) -> Result<String, String> {
    for ext in &["jpg", "jpeg", "png", "webp"] {
        if staging.join(format!("{}.{}", id, ext)).exists() {
            return Ok(ext.to_string());
        }
    }
    Err(format!("Staged file not found for {}", id))
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
                .write_image(&rgba, img.width(), img.height(), image::ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string())?;
            ("image/png", buf)
        }
        _ => {
            let mut buf = Vec::new();
            let rgb = img.to_rgb8();
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85);
            encoder.encode_image(&rgb).map_err(|e| e.to_string())?;
            ("image/jpeg", buf)
        }
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}
