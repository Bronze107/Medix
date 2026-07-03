use image::ImageEncoder;
use std::fs;
use std::io::Read;
use std::path::Path;
use tauri::{AppHandle, Manager};

use super::Media;
use crate::db;

/// Detect JPEG by magic bytes (FF D8 FF).
pub fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF]
}

/// Decode JPEG at reduced resolution using DCT-domain scaling.
pub fn decode_jpeg_fast(
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

    let dynamic = image::DynamicImage::ImageRgb8(
        image::RgbImage::from_raw(img.width as u32, img.height as u32, img.data)
            .ok_or("failed to construct image from decoded data")?,
    );
    Ok(dynamic)
}

/// Parse resize filter name. Defaults to Triangle.
pub fn parse_resize_filter(name: &str) -> image::imageops::FilterType {
    match name.to_lowercase().as_str() {
        "nearest" => image::imageops::FilterType::Nearest,
        "catmullrom" => image::imageops::FilterType::CatmullRom,
        "gaussian" => image::imageops::FilterType::Gaussian,
        "lanczos3" => image::imageops::FilterType::Lanczos3,
        _ => image::imageops::FilterType::Triangle,
    }
}

/// Generate a derivative by resizing/converting the source.
/// Creates a new media record + lineage link.
pub fn generate_derivative(
    app: &AppHandle,
    source_media_id: &str,
    source_path: &Path,
    _label: &str,
    format: &str,
    max_width: Option<u32>,
    max_height: Option<u32>,
    quality: u8,
    resize_filter: Option<&str>,
) -> Result<Media, Box<dyn std::error::Error>> {
    let app_dir = app.path().app_data_dir()?;
    let library_dir = app_dir.join("library");
    fs::create_dir_all(&library_dir)?;

    // Decode source
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

    // Resize
    let filter = parse_resize_filter(resize_filter.unwrap_or("triangle"));
    let resized = match (max_width, max_height) {
        (Some(max_w), Some(max_h)) => img.resize(max_w, max_h, filter),
        (Some(max_w), None) => img.resize(max_w, orig_h, filter),
        (None, Some(max_h)) => img.resize(orig_w, max_h, filter),
        (None, None) => img.clone(),
    };

    let id = ulid::Ulid::new().to_string();
    let ext = if format == "jpeg" { "jpg" } else { format };
    let file_name = format!("{}.{}", id, ext);
    let file_path = library_dir.join(&file_name);

    let (width, height) = (resized.width(), resized.height());
    let mut output = Vec::new();

    match ext {
        "jpg" | "jpeg" => {
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

    fs::write(&file_path, &output)?;
    let file_size = output.len() as i64;

    let source_str = file_path.to_string_lossy().replace('\\', "/");
    let now = chrono::Utc::now().to_rfc3339();

    let media = Media {
        id: id.clone(),
        source_path: Some(source_str),
        width: Some(width as i32),
        height: Some(height as i32),
        file_size: Some(file_size),
        created_at: None,
        modified_at: None,
        imported_at: now,
        source_url: None,
        page_url: None,
        source: Some("generated".into()),
        phash: None,
        sha256: None,
        deleted_at: None,
        display_variant_id: None,
        thumb_256: None,
        lqip: None,
        media_type: Some("image".into()),
        duration: None,
        video_codec: None,
        video_fps: None,
    };

    db::insert_media(app, &media)?;

    db::lineage_insert(app, source_media_id, &id, "generate", None)
        .map_err(|e| format!("lineage insert: {}", e))?;

    Ok(media)
}
