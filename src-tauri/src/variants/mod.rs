use serde::Serialize;
use std::fs;
use std::path::Path;
use image::ImageEncoder;
use tauri::{AppHandle, Manager};
use ulid::Ulid;

use crate::db;

#[derive(Debug, Clone, Serialize)]
pub struct Variant {
    pub id: String,
    pub media_id: String,
    pub preset_name: String,
    pub format: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub quality: Option<i32>,
    pub file_size: Option<i64>,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VariantPreset {
    pub name: String,
    pub label: String,
    pub format: String,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub quality: u8,
}

pub fn built_in_presets() -> Vec<VariantPreset> {
    vec![
        VariantPreset {
            name: "web_share".to_string(),
            label: "Web分享".to_string(),
            format: "jpeg".to_string(),
            max_width: Some(1080),
            max_height: None,
            quality: 75,
        },
        VariantPreset {
            name: "print".to_string(),
            label: "打印".to_string(),
            format: "png".to_string(),
            max_width: Some(2048),
            max_height: None,
            quality: 95,
        },
        VariantPreset {
            name: "dataset".to_string(),
            label: "训练数据集".to_string(),
            format: "jpeg".to_string(),
            max_width: Some(512),
            max_height: None,
            quality: 85,
        },
    ]
}

pub fn list_presets() -> Vec<VariantPreset> {
    built_in_presets()
}

pub fn generate_variant(
    app: &AppHandle,
    media_id: &str,
    source_path: &Path,
    preset_name: &str,
) -> Result<Variant, Box<dyn std::error::Error>> {
    let preset = built_in_presets()
        .into_iter()
        .find(|p| p.name == preset_name)
        .ok_or_else(|| format!("Unknown preset: {}", preset_name))?;

    let app_dir = app.path().app_data_dir()?;
    let variants_dir = app_dir.join("variants");
    fs::create_dir_all(&variants_dir)?;

    // Check if variant already exists
    if let Some(existing) = db::variant_get_by_media_and_preset(app, media_id, preset_name)? {
        let path = Path::new(&existing.file_path);
        if path.exists() {
            return Ok(existing);
        }
    }

    // Generate new variant
    let img = image::open(source_path)?;
    let (orig_w, orig_h) = (img.width(), img.height());

    let resized = match (preset.max_width, preset.max_height) {
        (Some(max_w), Some(max_h)) => img.resize(max_w, max_h, image::imageops::FilterType::Lanczos3),
        (Some(max_w), None) => img.resize(max_w, orig_h, image::imageops::FilterType::Lanczos3),
        (None, Some(max_h)) => img.resize(orig_w, max_h, image::imageops::FilterType::Lanczos3),
        (None, None) => img.clone(),
    };

    let id = Ulid::new().to_string();
    let file_name = format!("{}_{}.{}", media_id, preset_name, preset.format);
    let file_path = variants_dir.join(&file_name);

    let (width, height) = (resized.width(), resized.height());
    let mut output = Vec::new();

    match preset.format.as_str() {
        "jpeg" => {
            let rgb = resized.to_rgb8();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, preset.quality);
            encoder.encode_image(&rgb)?;
        }
        "png" => {
            let rgba = resized.to_rgba8();
            let encoder = image::codecs::png::PngEncoder::new(&mut output);
            encoder.write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)?;
        }
        _ => return Err(format!("Unsupported format: {}", preset.format).into()),
    }

    fs::write(&file_path, &output)?;
    let file_size = output.len() as i64;

    let variant = Variant {
        id: id.clone(),
        media_id: media_id.to_string(),
        preset_name: preset_name.to_string(),
        format: preset.format.clone(),
        width: Some(width as i32),
        height: Some(height as i32),
        quality: Some(preset.quality as i32),
        file_size: Some(file_size),
        file_path: file_path.to_string_lossy().replace('\\', "/"),
    };

    db::variant_insert(app, &variant)?;
    Ok(variant)
}
