use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Instant;
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
    pub label: Option<String>,
    pub source: Option<String>,
    pub media_type: Option<String>,
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub video_fps: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VariantPreset {
    pub name: String,
    pub label: String,
    pub format: String,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub quality: u8,
    pub resize_filter: String,
}

pub fn built_in_presets() -> Vec<VariantPreset> {
    vec![]
}

pub fn list_presets(app: &AppHandle) -> Result<Vec<VariantPreset>, Box<dyn std::error::Error>> {
    let mut presets = built_in_presets();
    let custom = db::variant_preset_list(app)?;
    presets.extend(custom);
    Ok(presets)
}

/// Detect JPEG by magic bytes (FF D8 FF).
fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF]
}

/// Decode JPEG at reduced resolution using DCT-domain scaling, avoiding full decode.
/// Uses 1/8, 1/4, or 1/2 scaling to get closest to `max_dim` without going under.
fn decode_jpeg_fast(
    path: &Path,
    max_dim: u32,
) -> Result<image::DynamicImage, Box<dyn std::error::Error>> {
    let jpeg_data = std::fs::read(path)?;
    let mut decoder = libjpeg_turbo_rs::Decoder::new(&jpeg_data)
        .map_err(|e| format!("jpeg decoder: {}", e))?;
    let (hdr_w, hdr_h) = {
        let header = decoder.header();
        (header.width, header.height)
    };
    let long_side = (hdr_w.max(hdr_h)) as u32;

    let scale: u32 = if long_side / 8 >= max_dim {
        8
    } else if long_side / 4 >= max_dim {
        4
    } else if long_side / 2 >= max_dim {
        2
    } else {
        1
    };

    decoder.set_scale(libjpeg_turbo_rs::ScalingFactor::new(1, scale));
    let img = decoder
        .decode_image()
        .map_err(|e| format!("jpeg decode: {}", e))?;

    println!(
        "[variant_generate] DCT decode 1/{}: {}x{} -> {}x{}",
        scale, hdr_w, hdr_h, img.width, img.height
    );

    let dynamic = image::DynamicImage::ImageRgb8(
        image::RgbImage::from_raw(img.width as u32, img.height as u32, img.data)
            .ok_or("failed to construct image from decoded data")?,
    );
    Ok(dynamic)
}

/// Parse a resize filter name into the image crate filter type.
/// Defaults to Triangle for unknown values.
pub fn parse_resize_filter(name: &str) -> image::imageops::FilterType {
    match name.to_lowercase().as_str() {
        "nearest" => image::imageops::FilterType::Nearest,
        "catmullrom" => image::imageops::FilterType::CatmullRom,
        "gaussian" => image::imageops::FilterType::Gaussian,
        "lanczos3" => image::imageops::FilterType::Lanczos3,
        _ => image::imageops::FilterType::Triangle,
    }
}

pub fn generate_variant(
    app: &AppHandle,
    media_id: &str,
    source_path: &Path,
    label: &str,
    format: &str,
    max_width: Option<u32>,
    max_height: Option<u32>,
    quality: u8,
    resize_filter: Option<&str>,
) -> Result<Variant, Box<dyn std::error::Error>> {
    let t_total = Instant::now();
    let log_phase = |phase: &str, t: &Instant| {
        println!(
            "[variant_generate] {} phase={} duration_ms={}",
            media_id,
            phase,
            t.elapsed().as_millis()
        );
    };

    let app_dir = app.path().app_data_dir()?;
    let variants_dir = app_dir.join("variants");
    fs::create_dir_all(&variants_dir)?;

    let t_decode = Instant::now();
    let mut magic = [0u8; 3];
    let source_is_jpeg = std::fs::File::open(source_path)
        .ok()
        .and_then(|mut f| f.read_exact(&mut magic).ok())
        .is_some()
        && is_jpeg(&magic);

    let target_long_side = match (max_width, max_height) {
        (Some(w), Some(h)) => w.max(h),
        (Some(w), None) => w,
        (None, Some(h)) => h,
        (None, None) => 0,
    };

    let img = if source_is_jpeg && target_long_side > 0 {
        decode_jpeg_fast(source_path, target_long_side)?
    } else {
        image::open(source_path)?
    };
    let (orig_w, orig_h) = (img.width(), img.height());
    log_phase("decode", &t_decode);

    let t_resize = Instant::now();
    let filter = parse_resize_filter(resize_filter.unwrap_or("triangle"));
    let resized = match (max_width, max_height) {
        (Some(max_w), Some(max_h)) => img.resize(max_w, max_h, filter),
        (Some(max_w), None) => img.resize(max_w, orig_h, filter),
        (None, Some(max_h)) => img.resize(orig_w, max_h, filter),
        (None, None) => img.clone(),
    };
    log_phase("resize", &t_resize);

    let id = Ulid::new().to_string();
    let file_name = format!("{}_{}.{}", media_id, id, format);
    let file_path = variants_dir.join(&file_name);

    let (width, height) = (resized.width(), resized.height());
    let mut output = Vec::new();

    let t_encode = Instant::now();
    match format {
        "jpeg" | "jpg" => {
            let rgb = resized.to_rgb8();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality);
            encoder.encode_image(&rgb)?;
        }
        "png" => {
            let rgba = resized.to_rgba8();
            let encoder = image::codecs::png::PngEncoder::new(&mut output);
            encoder.write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)?;
        }
        _ => return Err(format!("Unsupported format: {}", format).into()),
    }
    log_phase("encode", &t_encode);

    let t_write = Instant::now();
    fs::write(&file_path, &output)?;
    log_phase("write", &t_write);

    let file_size = output.len() as i64;

    let variant = Variant {
        id: id.clone(),
        media_id: media_id.to_string(),
        preset_name: String::new(),
        format: format.to_string(),
        width: Some(width as i32),
        height: Some(height as i32),
        quality: Some(quality as i32),
        file_size: Some(file_size),
        file_path: file_path.to_string_lossy().replace('\\', "/"),
        label: if label.is_empty() { None } else { Some(label.to_string()) },
        source: Some("generated".to_string()),
        media_type: None,
        duration: None,
        video_codec: None,
        video_fps: None,
    };

    let t_db = Instant::now();
    db::variant_insert(app, &variant)?;
    log_phase("db_insert", &t_db);

    log_phase("total", &t_total);
    Ok(variant)
}
