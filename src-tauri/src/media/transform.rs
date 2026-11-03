use image::ImageEncoder;
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

/// Decode a source image and resize it to fit within the given bounds.
///
/// JPEG sources use DCT-domain fast decoding when a resize target is set.
/// Shared by [`generate_derivative`] (persisted derivatives) and the dataset
/// export pipeline (`render_derivative_bytes`).
///
/// Note: like the rest of the pipeline, EXIF orientation is not applied.
pub fn load_and_resize(
    source_path: &Path,
    max_width: Option<u32>,
    max_height: Option<u32>,
    resize_filter: Option<&str>,
) -> Result<image::DynamicImage, Box<dyn std::error::Error>> {
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
    Ok(resized)
}

/// Encode a decoded image to JPEG or PNG bytes.
///
/// JPEG is encoded from RGB8 with the given quality; PNG from RGBA8 so alpha
/// is preserved. Shared by [`super::edit::save_image_as_derivative`] and the
/// dataset export pipeline.
pub fn encode_image_bytes(
    image: &image::DynamicImage,
    format: &str,
    quality: u8,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (width, height) = (image.width(), image.height());
    let mut output = Vec::new();
    match format {
        "jpg" | "jpeg" => {
            let rgb = image.to_rgb8();
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality);
            encoder.encode_image(&rgb)?;
        }
        "png" => {
            let rgba = image.to_rgba8();
            let encoder = image::codecs::png::PngEncoder::new(&mut output);
            encoder.write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)?;
        }
        _ => return Err(format!("Unsupported format: {}", format).into()),
    }
    Ok(output)
}

/// A rendered derivative: encoded bytes plus the output pixel dimensions
/// (after resize — needed for export metadata, which must describe the
/// produced file, not the source).
pub struct RenderedDerivative {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Decode + resize + encode a source image into in-memory derivative bytes.
///
/// Pure function: touches neither the database nor `library/`. The dataset
/// export pipeline uses this to render recipe rules directly into the export
/// directory.
pub fn render_derivative_bytes(
    source_path: &Path,
    format: &str,
    max_width: Option<u32>,
    max_height: Option<u32>,
    quality: u8,
    resize_filter: Option<&str>,
) -> Result<RenderedDerivative, Box<dyn std::error::Error>> {
    let resized = load_and_resize(source_path, max_width, max_height, resize_filter)?;
    let (width, height) = (resized.width(), resized.height());
    let bytes = encode_image_bytes(&resized, format, quality)?;
    Ok(RenderedDerivative {
        bytes,
        width,
        height,
    })
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
    let resized = load_and_resize(source_path, max_width, max_height, resize_filter)?;

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

#[cfg(test)]
mod transform_tests {
    use super::*;

    fn temp_jpeg(w: u32, h: u32) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("src.jpg");
        let img = image::DynamicImage::new_rgb8(w, h);
        img.save_with_format(&path, image::ImageFormat::Jpeg)
            .expect("save jpeg");
        (dir, path)
    }

    #[test]
    fn test_render_derivative_bytes_resizes_jpeg() {
        let (_dir, path) = temp_jpeg(800, 600);
        let out =
            render_derivative_bytes(&path, "jpeg", Some(400), None, 85, None).expect("render");
        assert_eq!((out.width, out.height), (400, 300));
        assert!(is_jpeg(&out.bytes), "output should be JPEG");
    }

    #[test]
    fn test_render_derivative_bytes_png_output() {
        let (_dir, path) = temp_jpeg(800, 600);
        let out =
            render_derivative_bytes(&path, "png", Some(200), Some(200), 85, None).expect("render");
        // resize fits within bounds and keeps the aspect ratio: 800x600 → 200x150.
        assert_eq!((out.width, out.height), (200, 150));
        assert!(
            out.bytes.starts_with(&[0x89, b'P', b'N', b'G']),
            "output should be PNG"
        );
    }

    #[test]
    fn test_render_derivative_bytes_no_bounds_keeps_size() {
        let (_dir, path) = temp_jpeg(320, 240);
        let out = render_derivative_bytes(&path, "jpeg", None, None, 85, None).expect("render");
        assert_eq!((out.width, out.height), (320, 240));
    }

    #[test]
    fn test_render_derivative_bytes_rejects_bad_format() {
        let (_dir, path) = temp_jpeg(100, 100);
        let err = render_derivative_bytes(&path, "webp", None, None, 85, None);
        assert!(err.is_err(), "unsupported format must error");
    }

    #[test]
    fn test_encode_image_bytes_png_keeps_alpha() {
        let img = image::DynamicImage::new_rgba8(10, 10);
        let bytes = encode_image_bytes(&img, "png", 85).expect("encode");
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn test_parse_resize_filter_names() {
        assert!(matches!(
            parse_resize_filter("lanczos3"),
            image::imageops::FilterType::Lanczos3
        ));
        // Unknown names fall back to Triangle.
        assert!(matches!(
            parse_resize_filter("nonsense"),
            image::imageops::FilterType::Triangle
        ));
    }
}
