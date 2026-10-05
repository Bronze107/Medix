//! 图像编辑产物的落库：裁剪（后续还要接画布导出）。
//!
//! 与 [`crate::media::transform::generate_derivative`] 共用同一套
//! 「写 `library/` → 插 media 行 → 记 lineage」流程（见 [`save_image_as_derivative`]），
//! 这里额外负责 EXIF 方向归一与裁剪区域校验。

use std::fs;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader};
use tauri::{AppHandle, Manager};

use super::Media;
use crate::db;

/// 按 EXIF 方向解码图片。
///
/// `image::open` **不会**应用 EXIF 方向，而浏览器渲染 `<img>` 时默认会
/// （`image-orientation: from-image`）。两者不一致会让「前端看到的坐标」与
/// 「Rust 操作的像素」错位 —— 对裁剪是致命的：会把框选区域裁到别处。
pub fn open_oriented(path: &Path) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// 校验并夹取裁剪矩形，返回 `(x, y, w, h)`；完全不合法时返回 `None`。
///
/// **必须先校验再交给 `crop_imm`**：`image` 0.25 的 `crop_imm` 对越界区域是
/// 断言失败（panic），不能拿它当边界检查。超出右下边界的部分夹取到图像边缘，
/// 起点越界或零宽高则直接判非法。
pub fn clamp_crop_rect(
    img_w: u32,
    img_h: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<(u32, u32, u32, u32)> {
    if w == 0 || h == 0 {
        return None;
    }
    if x >= img_w || y >= img_h {
        return None;
    }
    // checked_add：x + w 溢出 u32 说明矩形本身荒谬，判非法
    let x1 = x.checked_add(w)?.min(img_w);
    let y1 = y.checked_add(h)?.min(img_h);
    // 夹取后仍无有效尺寸（理论上不会发生，防御性保留）
    if x1 <= x || y1 <= y {
        return None;
    }
    Some((x, y, x1 - x, y1 - y))
}

/// 组装一条衍生 `media` 记录（不落库）。
fn derivative_row(
    id: &str,
    file_path: &Path,
    width: u32,
    height: u32,
    file_size: i64,
    source_tag: &str,
) -> Media {
    Media {
        id: id.to_string(),
        source_path: Some(file_path.to_string_lossy().replace('\\', "/")),
        width: Some(width as i32),
        height: Some(height as i32),
        file_size: Some(file_size),
        created_at: None,
        modified_at: None,
        imported_at: chrono::Utc::now().to_rfc3339(),
        source_url: None,
        page_url: None,
        source: Some(source_tag.into()),
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
    }
}

/// 把一张已解码的图编码写入 `library/`，插 media 行并记 lineage，返回新记录。
///
/// 从 `generate_derivative` 抽出的公共尾部：两种输出格式的编码方式保持不变
/// （PNG 走 RGBA 以保留 alpha）。
pub fn save_image_as_derivative(
    app: &AppHandle,
    parent_media_id: &str,
    image: &DynamicImage,
    format: &str,
    quality: u8,
    relation: &str,
    source_tag: &str,
) -> Result<Media, Box<dyn std::error::Error>> {
    let app_dir = app.path().app_data_dir()?;
    let library_dir = app_dir.join("library");
    fs::create_dir_all(&library_dir)?;

    let id = ulid::Ulid::new().to_string();
    let ext = if format == "jpeg" { "jpg" } else { format };
    let file_path = library_dir.join(format!("{}.{}", id, ext));

    let (width, height) = (image.width(), image.height());
    let mut output = Vec::new();
    match ext {
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

    fs::write(&file_path, &output)?;
    let media = derivative_row(
        &id,
        &file_path,
        width,
        height,
        output.len() as i64,
        source_tag,
    );

    db::insert_media(app, &media)?;
    db::lineage_insert(app, parent_media_id, &id, relation, None)
        .map_err(|e| format!("lineage insert: {}", e))?;

    Ok(media)
}

/// 把画布导出的 PNG 字节**原样**写入 `library/` 并存为新版本。
///
/// 刻意不重新编码：画布输出的就是 PNG（无损），重编码只会白费 CPU 并可能
/// 改变色彩。尺寸仅用于建 media 行。
pub fn save_png_bytes_as_derivative(
    app: &AppHandle,
    parent_media_id: &str,
    png_bytes: &[u8],
    relation: &str,
    source_tag: &str,
) -> Result<Media, Box<dyn std::error::Error>> {
    let app_dir = app.path().app_data_dir()?;
    let library_dir = app_dir.join("library");
    fs::create_dir_all(&library_dir)?;

    let img = image::load_from_memory(png_bytes)?;
    let (width, height) = (img.width(), img.height());

    let id = ulid::Ulid::new().to_string();
    let file_path = library_dir.join(format!("{}.png", id));
    fs::write(&file_path, png_bytes)?;

    let media = derivative_row(
        &id,
        &file_path,
        width,
        height,
        png_bytes.len() as i64,
        source_tag,
    );

    db::insert_media(app, &media)?;
    db::lineage_insert(app, parent_media_id, &id, relation, None)
        .map_err(|e| format!("lineage insert: {}", e))?;

    Ok(media)
}

/// 裁剪原图并存为新版本。裁剪本身无损（直接切像素），编码沿用新格式的常规方式。
pub fn crop_to_derivative(
    app: &AppHandle,
    source_media_id: &str,
    source_path: &Path,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    format: &str,
    quality: u8,
) -> Result<Media, Box<dyn std::error::Error>> {
    let img = open_oriented(source_path)?;
    let (img_w, img_h) = (img.width(), img.height());
    let (cx, cy, cw, ch) = clamp_crop_rect(img_w, img_h, x, y, width, height).ok_or_else(|| {
        format!(
            "裁剪区域超出图像边界：图像 {}x{}，请求 ({}, {}) {}x{}",
            img_w, img_h, x, y, width, height
        )
    })?;
    let cropped = img.crop_imm(cx, cy, cw, ch);
    save_image_as_derivative(
        app,
        source_media_id,
        &cropped,
        format,
        quality,
        "crop",
        "edited",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_crop_rect_normal() {
        assert_eq!(
            clamp_crop_rect(100, 200, 10, 20, 30, 40),
            Some((10, 20, 30, 40))
        );
        // 贴边整图
        assert_eq!(
            clamp_crop_rect(100, 200, 0, 0, 100, 200),
            Some((0, 0, 100, 200))
        );
        // 1x1
        assert_eq!(
            clamp_crop_rect(100, 200, 99, 199, 1, 1),
            Some((99, 199, 1, 1))
        );
    }

    #[test]
    fn test_clamp_crop_rect_clamps_to_edges() {
        // 右下越界 → 夹到边缘，而不是报错
        assert_eq!(
            clamp_crop_rect(100, 200, 90, 190, 50, 50),
            Some((90, 190, 10, 10))
        );
        assert_eq!(
            clamp_crop_rect(100, 200, 0, 0, 1000, 1000),
            Some((0, 0, 100, 200))
        );
    }

    #[test]
    fn test_clamp_crop_rect_rejects_invalid() {
        // 零宽/零高
        assert_eq!(clamp_crop_rect(100, 200, 10, 10, 0, 10), None);
        assert_eq!(clamp_crop_rect(100, 200, 10, 10, 10, 0), None);
        // 起点越界
        assert_eq!(clamp_crop_rect(100, 200, 100, 0, 10, 10), None);
        assert_eq!(clamp_crop_rect(100, 200, 0, 200, 10, 10), None);
        assert_eq!(clamp_crop_rect(100, 200, u32::MAX, u32::MAX, 10, 10), None);
    }

    #[test]
    fn test_clamp_crop_rect_overflow_does_not_panic() {
        // x + w 溢出 u32：必须判非法而不是 panic/wrap
        assert_eq!(clamp_crop_rect(100, 200, 10, 10, u32::MAX, 10), None);
        assert_eq!(clamp_crop_rect(100, 200, 10, 10, 10, u32::MAX), None);
        // 退化图像
        assert_eq!(clamp_crop_rect(0, 0, 0, 0, 1, 1), None);
    }
}
