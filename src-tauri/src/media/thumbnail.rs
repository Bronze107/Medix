use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager};

const THUMB_SIZES: &[(u32, &str)] = &[(256, "256"), (512, "512")];

pub fn generate_thumbnails(
    app: &AppHandle,
    media_id: &str,
    source_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let app_dir = app.path().app_data_dir()?;
    let thumbs_dir = app_dir.join("thumbnails");
    fs::create_dir_all(&thumbs_dir)?;

    let img = image::open(source_path)?;

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
