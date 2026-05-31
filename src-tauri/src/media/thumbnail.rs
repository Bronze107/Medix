use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager};

const THUMB_SIZES: &[(u32, &str)] = &[(256, "256")];

pub fn generate_thumbnails(
    app: &AppHandle,
    media_id: &str,
    source_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let img = image::open(source_path)?;
    generate_thumbnails_from_image(app, media_id, &img)
}

/// Generate thumbnails from an already-decoded image, avoiding re-decode.
pub fn generate_thumbnails_from_image(
    app: &AppHandle,
    media_id: &str,
    img: &image::DynamicImage,
) -> Result<(), Box<dyn std::error::Error>> {
    let app_dir = app.path().app_data_dir()?;
    let thumbs_dir = app_dir.join("thumbnails");
    fs::create_dir_all(&thumbs_dir)?;

    for (size, suffix) in THUMB_SIZES {
        let thumb_path = thumbs_dir.join(format!("{}_{}.jpg", media_id, suffix));
        if thumb_path.exists() {
            continue;
        }

        let thumb = img.resize(*size, *size, image::imageops::FilterType::Lanczos3);
        let rgb_thumb = thumb.to_rgb8();
        let mut output = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 85);
        encoder.encode_image(&rgb_thumb)?;
        fs::write(&thumb_path, output)?;
    }

    Ok(())
}
