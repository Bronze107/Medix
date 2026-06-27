use std::fs;
use std::path::Path;
use tauri::{command, AppHandle, Emitter, Manager};

use crate::db;
use crate::media;
use crate::variants::{list_presets, generate_variant, Variant, VariantPreset};

fn resolve_source_path(app: &AppHandle, media_id: &str) -> Result<std::path::PathBuf, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let library_dir = app_dir.join("library");
    for entry in fs::read_dir(&library_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&format!("{}.", media_id)) {
            return Ok(entry.path());
        }
    }
    Err("Source file not found in library".to_string())
}

#[command]
pub fn variant_list(app: AppHandle, media_id: String) -> Result<Vec<Variant>, String> {
    db::variant_list(&app, &media_id).map_err(|e| e.to_string())
}

#[command]
pub async fn variant_generate(
    app: AppHandle,
    media_id: String,
    label: String,
    format: String,
    max_width: Option<u32>,
    max_height: Option<u32>,
    quality: u8,
) -> Result<Variant, String> {
    tokio::task::spawn_blocking(move || {
        let source_path = resolve_source_path(&app, &media_id)?;
        let variant = generate_variant(
            &app, &media_id, &source_path,
            &label, &format, max_width, max_height, quality,
        )
        .map_err(|e| e.to_string())?;
        // Generate thumbnail for the variant
        if let Err(e) = media::thumbnail::generate_variant_thumbnail(
            &app, &variant.id, Path::new(&variant.file_path),
        ) {
            eprintln!("[variant] thumbnail failed for {}: {}", variant.id, e);
        }
        Ok(variant)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[command]
pub fn variant_import(
    app: AppHandle,
    media_id: String,
    source_path: String,
) -> Result<Variant, String> {
    let src = Path::new(&source_path);
    if !src.exists() {
        return Err("Source file not found".to_string());
    }

    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let is_image = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp");
    let is_video = crate::media::video_metadata::VIDEO_EXTENSIONS.contains(&ext.as_str());
    if !is_image && !is_video {
        return Err(format!("Unsupported file type: {}", ext));
    }

    let file_size = src.metadata().map_err(|e| e.to_string())?.len() as i64;

    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let versions_dir = app_dir.join("variants");
    fs::create_dir_all(&versions_dir).map_err(|e| e.to_string())?;

    let id = ulid::Ulid::new().to_string();
    let file_name = format!("{}_{}.{}", media_id, id, ext);
    let dest = versions_dir.join(&file_name);
    fs::copy(src, &dest).map_err(|e| e.to_string())?;

    let label = src
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());

    let (width, height, media_type, duration, video_codec, video_fps) = if is_video {
        let meta = crate::media::video_metadata::extract_metadata(src)
            .map_err(|e| format!("ffprobe failed: {}", e))?;
        (
            Some(meta.width),
            Some(meta.height),
            Some("video".to_string()),
            meta.duration,
            meta.video_codec,
            meta.video_fps,
        )
    } else {
        let img = image::open(src).map_err(|e| e.to_string())?;
        (
            Some(img.width() as i32),
            Some(img.height() as i32),
            Some("image".to_string()),
            None,
            None,
            None,
        )
    };

    let variant = Variant {
        id: id.clone(),
        media_id: media_id.clone(),
        preset_name: String::new(),
        format: ext,
        width,
        height,
        quality: None,
        file_size: Some(file_size),
        file_path: dest.to_string_lossy().replace('\\', "/"),
        label,
        source: Some("imported".to_string()),
        media_type,
        duration,
        video_codec,
        video_fps,
    };

    db::variant_insert(&app, &variant).map_err(|e| e.to_string())?;
    // Generate thumbnail for the variant
    if is_video {
        if let Err(e) = crate::media::video_thumbnail::generate_video_thumbnail(
            &app, &variant.id, Path::new(&variant.file_path), variant.duration,
        ) {
            eprintln!("[variant] video thumbnail failed for imported {}: {}", variant.id, e);
        }
    } else if let Err(e) = media::thumbnail::generate_variant_thumbnail(
        &app, &variant.id, Path::new(&variant.file_path),
    ) {
        eprintln!("[variant] thumbnail failed for imported {}: {}", variant.id, e);
    }
    Ok(variant)
}

#[command]
pub fn variant_delete(app: AppHandle, id: String) -> Result<(), String> {
    // Delete file first
    if let Ok(Some(v)) = db::variant_get_by_id(&app, &id) {
        let _ = fs::remove_file(&v.file_path);
    }
    db::variant_delete(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn variant_presets() -> Vec<VariantPreset> {
    list_presets()
}

#[command]
pub fn variant_annotate(
    app: AppHandle,
    media_id: String,
    variant_id: String,
) -> Result<(), String> {
    let variant = db::variant_get_by_id(&app, &variant_id)
        .map_err(|e| e.to_string())?
        .ok_or("Variant not found")?;
    let file_path = std::path::PathBuf::from(&variant.file_path);
    if !file_path.exists() {
        return Err("Variant file not found on disk".to_string());
    }
    let queue = app.state::<crate::ai::AiQueue>();

    // Dispatch the right task based on media type (video variants need video pipeline)
    if variant.media_type.as_deref() == Some("video") {
        let duration = variant.duration.unwrap_or(0.0);
        queue
            .send(crate::ai::AiTask::GenerateVideoCaption {
                media_id: media_id.clone(),
                video_path: file_path,
                duration_secs: duration,
                variant_id: Some(variant_id),
            })
            .map_err(|e| e.to_string())?;
    } else {
        queue
            .send(crate::ai::AiTask::GenerateCaption {
                media_id: media_id.clone(),
                image_path: file_path,
                variant_id: Some(variant_id),
            })
            .map_err(|e| e.to_string())?;
    }
    let _ = app.emit(
        "ai-task-done",
        crate::ai::AiTaskProgress {
            remaining: queue.pending_count(),
        },
    );
    Ok(())
}

#[command]
pub fn media_set_display_variant(
    app: AppHandle,
    media_id: String,
    variant_id: Option<String>,
) -> Result<(), String> {
    db::media_set_display_variant(&app, &media_id, variant_id.as_deref())
        .map_err(|e| e.to_string())
}

#[command]
pub fn media_reset_all_display_variants(app: AppHandle) -> Result<u64, String> {
    db::media_reset_all_display_variants(&app).map_err(|e| e.to_string())
}
