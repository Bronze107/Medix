use std::io::Read;
use std::path::Path;
use tauri::AppHandle;

use super::Media;

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
    let mut decoder =
        libjpeg_turbo_rs::Decoder::new(&jpeg_data).map_err(|e| format!("jpeg decoder: {}", e))?;
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

    super::edit::save_image_as_derivative(
        app,
        source_media_id,
        &resized,
        format,
        quality,
        "generate",
        "generated",
    )
}
