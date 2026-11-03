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

/// Read a JPEG's dimensions from the header without decoding pixels.
fn jpeg_dimensions(path: &Path) -> Option<(u32, u32)> {
    let data = std::fs::read(path).ok()?;
    let mut decoder = libjpeg_turbo_rs::Decoder::new(&data).ok()?;
    let header = decoder.header();
    Some((header.width as u32, header.height as u32))
}

/// Scale a decoded image down to a megapixel budget, preserving aspect ratio.
///
/// Images already within the budget pass through unchanged; `None`/non-positive
/// budgets mean "no limit".
fn resize_to_megapixel_budget(
    img: image::DynamicImage,
    max_megapixels: Option<f64>,
    filter: image::imageops::FilterType,
) -> image::DynamicImage {
    let Some(mp) = max_megapixels.filter(|m| *m > 0.0) else {
        return img;
    };
    let budget = mp * 1_000_000.0;
    let (w, h) = (img.width() as f64, img.height() as f64);
    if w * h <= budget {
        return img;
    }
    let scale = (budget / (w * h)).sqrt();
    let nw = ((w * scale).round() as u32).max(1);
    let nh = ((h * scale).round() as u32).max(1);
    img.resize(nw, nh, filter)
}

/// Decode + resize + encode a source image into in-memory derivative bytes.
///
/// The size limit is a megapixel budget: the image is scaled down (aspect
/// ratio preserved) until its pixel count fits. Pure function: touches
/// neither the database nor `library/`. The dataset export pipeline uses
/// this to render recipe rules directly into the export directory.
pub fn render_derivative_bytes(
    source_path: &Path,
    format: &str,
    max_megapixels: Option<f64>,
    quality: u8,
    resize_filter: Option<&str>,
) -> Result<RenderedDerivative, Box<dyn std::error::Error>> {
    let mut magic = [0u8; 3];
    let source_is_jpeg = std::fs::File::open(source_path)
        .ok()
        .and_then(|mut f| f.read_exact(&mut magic).ok())
        .is_some()
        && is_jpeg(&magic);

    // Fast-path target: the DCT decoder wants a long-side bound, but a
    // megapixel budget only fixes the pixel count. Estimate the long side
    // from the (cheap) JPEG header: for pixel budget B and aspect ratio the
    // budget scales both axes by sqrt(B / (w*h)).
    let fast_target = match (source_is_jpeg, max_megapixels) {
        (true, Some(mp)) if mp > 0.0 => jpeg_dimensions(source_path).and_then(|(w, h)| {
            let px = w as f64 * h as f64;
            let budget = mp * 1_000_000.0;
            if px > budget {
                let scale = (budget / px).sqrt();
                // ceil: never decode smaller than the budget allows; the
                // exact resize below does the final say.
                Some((((w.max(h)) as f64 * scale).ceil() as u32).max(1))
            } else {
                None
            }
        }),
        _ => None,
    };

    let img = match fast_target {
        Some(target) => decode_jpeg_fast(source_path, target)?,
        None => image::open(source_path)?,
    };

    let filter = parse_resize_filter(resize_filter.unwrap_or("triangle"));
    let resized = resize_to_megapixel_budget(img, max_megapixels, filter);
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
    fn test_render_derivative_bytes_resizes_to_megapixel_budget() {
        let (_dir, path) = temp_jpeg(800, 600); // 0.48 MP
        let out = render_derivative_bytes(&path, "jpeg", Some(0.1), 85, None).expect("render");
        // 0.1 MP 预算：800x600=480000px → 缩到 ~100000px，保持 4:3
        assert!(
            out.width * out.height <= 110_000,
            "{}x{}",
            out.width,
            out.height
        );
        assert!(
            out.width * out.height >= 90_000,
            "{}x{}",
            out.width,
            out.height
        );
        // 宽高各自四舍五入会有 ±1px 的比例误差，用容差断言
        let ratio = out.width as f64 / out.height as f64;
        assert!(
            (ratio - 4.0f64 / 3.0).abs() < 0.01,
            "aspect ratio preserved: {}x{}",
            out.width,
            out.height
        );
        assert!(is_jpeg(&out.bytes), "output should be JPEG");
    }

    #[test]
    fn test_render_derivative_bytes_png_output() {
        let (_dir, path) = temp_jpeg(800, 600);
        let out = render_derivative_bytes(&path, "png", Some(0.05), 85, None).expect("render");
        // 预算 50000px：缩到约 204x153
        assert!(
            out.width * out.height <= 55_000,
            "{}x{}",
            out.width,
            out.height
        );
        assert!(
            out.bytes.starts_with(&[0x89, b'P', b'N', b'G']),
            "output should be PNG"
        );
    }

    #[test]
    fn test_render_derivative_bytes_no_budget_keeps_size() {
        let (_dir, path) = temp_jpeg(320, 240);
        let out = render_derivative_bytes(&path, "jpeg", None, 85, None).expect("render");
        assert_eq!((out.width, out.height), (320, 240));
    }

    #[test]
    fn test_render_derivative_bytes_budget_above_source_keeps_size() {
        let (_dir, path) = temp_jpeg(320, 240); // 0.0768 MP
        let out = render_derivative_bytes(&path, "jpeg", Some(1.0), 85, None).expect("render");
        assert_eq!(
            (out.width, out.height),
            (320, 240),
            "budget above source must not upscale"
        );
    }

    #[test]
    fn test_render_derivative_bytes_rejects_bad_format() {
        let (_dir, path) = temp_jpeg(100, 100);
        let err = render_derivative_bytes(&path, "webp", None, 85, None);
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
