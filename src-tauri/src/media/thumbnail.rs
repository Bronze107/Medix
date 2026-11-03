use base64::Engine;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager};

/// 高档位缩略图（最长边 px）。256 是基础档，始终生成（实际尺寸钳制到原图内）。
pub const HIGH_TIERS: &[u32] = &[512, 1024];

/// 基础档位。
pub const BASE_TIER: u32 = 256;

/// 该媒体应生成的全部档位：基础档恒在，高档位仅当不超过原图最长边时生成
/// （缩略图尺寸不得超过原图——禁止放大）。
pub fn applicable_tiers(longest_edge: u32) -> Vec<u32> {
    let mut tiers = vec![BASE_TIER];
    for t in HIGH_TIERS {
        if *t <= longest_edge {
            tiers.push(*t);
        }
    }
    tiers
}

/// 根据渲染所需像素（最长边）选择档位，规则与前端 `pickThumbnailTier` 一致：
/// 需求 ≤ 基础档直接用基础档；否则在不超过原图最长边的高档位中选最小的
/// 满足档，没有则退到最大的可用高档位；原图太小没有高档位时回到基础档。
pub fn pick_tier(requested_px: u32, longest_edge: u32) -> u32 {
    if requested_px <= BASE_TIER {
        return BASE_TIER;
    }
    let candidates: Vec<u32> = HIGH_TIERS
        .iter()
        .copied()
        .filter(|t| *t <= longest_edge)
        .collect();
    match candidates.iter().find(|t| **t >= requested_px) {
        Some(t) => *t,
        None => candidates.last().copied().unwrap_or(BASE_TIER),
    }
}

/// 档位缩略图文件名：`{media_id}_{tier}.jpg`。
pub fn tier_file_name(media_id: &str, tier: u32) -> String {
    format!("{}_{}.jpg", media_id, tier)
}

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
    generate_thumbnails_in_dir(&thumbs_dir, media_id, img)
}

/// Generate all applicable thumbnail tiers into `thumbs_dir`.
///
/// 每个档位的实际尺寸钳制到原图最长边以内（不放大）；已存在的档位跳过，
/// 因此对老库补齐高档位时天然幂等。
pub fn generate_thumbnails_in_dir(
    thumbs_dir: &Path,
    media_id: &str,
    img: &image::DynamicImage,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(thumbs_dir)?;

    let longest = img.width().max(img.height()).max(1);
    for tier in applicable_tiers(longest) {
        let thumb_path = thumbs_dir.join(tier_file_name(media_id, tier));
        if thumb_path.exists() {
            continue;
        }

        let edge = tier.min(longest);
        let thumb = img.resize(edge, edge, image::imageops::FilterType::Triangle);
        let rgb_thumb = thumb.to_rgb8();
        let mut output = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 85);
        encoder.encode_image(&rgb_thumb)?;
        // 先写临时文件再原子重命名，避免 asset:// 读到写了一半的文件
        let tmp_path = thumbs_dir.join(format!("{}.tmp", tier_file_name(media_id, tier)));
        fs::write(&tmp_path, output)?;
        fs::rename(&tmp_path, &thumb_path)?;
    }

    Ok(())
}

/// Generate a thumbnail (all applicable tiers) for a variant file from an
/// already-decoded image.
pub fn generate_variant_thumbnail_from_image(
    app: &AppHandle,
    variant_id: &str,
    img: &image::DynamicImage,
) -> Result<(), Box<dyn std::error::Error>> {
    let app_dir = app.path().app_data_dir()?;
    let thumbs_dir = app_dir.join("thumbnails");
    generate_variant_thumbnail_in_dir(&thumbs_dir, variant_id, img)
}

/// Variant counterpart of [`generate_thumbnails_in_dir`].
pub fn generate_variant_thumbnail_in_dir(
    thumbs_dir: &Path,
    variant_id: &str,
    img: &image::DynamicImage,
) -> Result<(), Box<dyn std::error::Error>> {
    // Variant 缩略图与普通媒体共用命名空间和档位规则
    generate_thumbnails_in_dir(thumbs_dir, variant_id, img)
}

/// Generate a thumbnail for a variant file.
/// Uses the variant's ID as the thumbnail filename.
pub fn generate_variant_thumbnail(
    app: &AppHandle,
    variant_id: &str,
    source_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let img = image::open(source_path)?;
    generate_variant_thumbnail_from_image(app, variant_id, &img)
}

/// Generate a 20px JPEG placeholder and return as base64 data URL (~300 bytes).
pub fn generate_lqip(img: &image::DynamicImage) -> String {
    let thumb = img.resize_exact(20, 20, image::imageops::FilterType::Nearest);
    let rgb = thumb.to_rgb8();
    let mut output = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 30);
    if encoder.encode_image(&rgb).is_err() {
        return String::new();
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&output);
    format!("data:image/jpeg;base64,{}", b64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_applicable_tiers_clamp_to_original() {
        // 大图：全档位
        assert_eq!(applicable_tiers(4000), vec![256, 512, 1024]);
        // 中图：只到 512
        assert_eq!(applicable_tiers(600), vec![256, 512]);
        // 小于 512 但大于 256：只有基础档
        assert_eq!(applicable_tiers(300), vec![256]);
        // 小于基础档：只有基础档（生成时钳制到原图尺寸，不放大）
        assert_eq!(applicable_tiers(100), vec![256]);
    }

    #[test]
    fn test_pick_tier_basic_requests_use_base_tier() {
        assert_eq!(pick_tier(1, 4000), 256);
        assert_eq!(pick_tier(256, 4000), 256);
        assert_eq!(pick_tier(200, 4000), 256);
    }

    #[test]
    fn test_pick_tier_selects_smallest_sufficient_tier() {
        assert_eq!(pick_tier(257, 4000), 512);
        assert_eq!(pick_tier(512, 4000), 512);
        assert_eq!(pick_tier(513, 4000), 1024);
        assert_eq!(pick_tier(2000, 4000), 1024);
    }

    #[test]
    fn test_pick_tier_respects_original_size() {
        // 原图 600px：512 可用，1024 不可用
        assert_eq!(pick_tier(800, 600), 512);
        // 原图 300px：没有可用高档位，退回基础档
        assert_eq!(pick_tier(800, 300), 256);
        // 需求超过最大档位时退到最大可用档
        assert_eq!(pick_tier(9999, 4000), 1024);
        assert_eq!(pick_tier(9999, 600), 512);
    }

    #[test]
    fn test_generate_in_dir_clamps_to_original_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        // 100x80 小图
        let img = image::DynamicImage::new_rgb8(100, 80);
        generate_thumbnails_in_dir(dir.path(), "01SMALL", &img).expect("generate");

        // 只生成基础档，且尺寸不超过原图
        assert!(dir.path().join("01SMALL_256.jpg").exists());
        assert!(!dir.path().join("01SMALL_512.jpg").exists());
        let decoded = image::open(dir.path().join("01SMALL_256.jpg")).expect("open thumb");
        assert!(decoded.width() <= 100 && decoded.height() <= 80);
    }

    #[test]
    fn test_generate_in_dir_multi_tier_and_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let img = image::DynamicImage::new_rgb8(2000, 1000);
        generate_thumbnails_in_dir(dir.path(), "01BIG", &img).expect("generate");

        for tier in [256, 512, 1024] {
            let p = dir.path().join(format!("01BIG_{}.jpg", tier));
            assert!(p.exists(), "tier {} missing", tier);
            let decoded = image::open(&p).expect("open thumb");
            assert_eq!(decoded.width().max(decoded.height()), tier);
        }

        // 幂等：重复生成不报错（已存在的跳过）
        generate_thumbnails_in_dir(dir.path(), "01BIG", &img).expect("regenerate");
    }
}
