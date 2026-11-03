use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

use super::thumbnail::{applicable_tiers, tier_file_name, BASE_TIER};

/// Generate a single-tier thumbnail from a video file using ffmpeg.
/// Tries multiple timestamps in order: 10% of duration -> 1.0s -> 50% -> first frame.
///
/// 抽帧尺寸钳制在 `min(iw, size)` / `min(ih, size)` 内，不会放大原图。
pub fn generate_video_thumbnail_sized(
    app: &AppHandle,
    media_id: &str,
    source_path: &Path,
    duration_secs: Option<f64>,
    size: u32,
) -> Result<PathBuf, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumbs_dir = app_dir.join("thumbnails");
    std::fs::create_dir_all(&thumbs_dir).map_err(|e| e.to_string())?;
    let thumb_path = thumbs_dir.join(tier_file_name(media_id, size));

    let timestamps: Vec<String> = {
        let mut ts = Vec::new();
        // 10% position
        if let Some(d) = duration_secs {
            if d > 0.1 {
                ts.push(format!("{:.3}", d * 0.1));
            }
        }
        ts.push("1.0".to_string());
        if let Some(d) = duration_secs {
            if d > 1.0 {
                ts.push(format!("{:.3}", d * 0.5));
            }
        }
        ts.push("0.0".to_string());
        ts
    };

    let scale_filter = format!(
        "scale='min(iw,{size})':'min(ih,{size})':force_original_aspect_ratio=decrease",
        size = size
    );

    for (i, ts) in timestamps.iter().enumerate() {
        let result = Command::new(crate::media::video_metadata::find_ffmpeg())
            .args(["-ss", ts, "-i"])
            .arg(source_path)
            .args(["-frames:v", "1", "-vf", &scale_filter, "-q:v", "3", "-y"])
            .arg(&thumb_path)
            .output();

        match result {
            Ok(output) if output.status.success() && thumb_path.exists() => {
                return Ok(thumb_path);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "[video_thumbnail] attempt {}/{} (t={}) failed: {}",
                    i + 1,
                    timestamps.len(),
                    ts,
                    stderr.lines().last().unwrap_or("unknown error")
                );
            }
            Err(e) => {
                eprintln!(
                    "[video_thumbnail] attempt {}/{} (t={}) ffmpeg error: {}",
                    i + 1,
                    timestamps.len(),
                    ts,
                    e
                );
            }
        }
    }

    // All attempts failed
    if thumb_path.exists() {
        Ok(thumb_path)
    } else {
        Err("All thumbnail extraction attempts failed".to_string())
    }
}

/// Generate the base 256px thumbnail. Kept for callers that only need the
/// poster frame (legacy entry point).
pub fn generate_video_thumbnail(
    app: &AppHandle,
    media_id: &str,
    source_path: &Path,
    duration_secs: Option<f64>,
) -> Result<PathBuf, String> {
    generate_video_thumbnail_sized(app, media_id, source_path, duration_secs, BASE_TIER)
}

/// Generate all applicable thumbnail tiers for a video.
///
/// `longest_edge` 是视频原生的最长边（来自 ffprobe 元数据）：高档位仅当不
/// 超过它时生成；为 `None` 时按只有基础档处理（保守，不放大）。单档失败
/// 不阻塞其他档位——返回时只要基础档成功即视为成功。
pub fn generate_video_thumbnails(
    app: &AppHandle,
    media_id: &str,
    source_path: &Path,
    duration_secs: Option<f64>,
    longest_edge: Option<u32>,
) -> Result<(), String> {
    let longest = longest_edge.unwrap_or(0);
    let mut base_ok = false;
    let mut last_err = String::new();

    for tier in applicable_tiers(longest) {
        match generate_video_thumbnail_sized(app, media_id, source_path, duration_secs, tier) {
            Ok(_) => {
                if tier == BASE_TIER {
                    base_ok = true;
                }
            }
            Err(e) => {
                eprintln!("[video_thumbnail] tier {} failed: {}", tier, e);
                last_err = e;
            }
        }
    }

    if base_ok {
        Ok(())
    } else {
        Err(last_err)
    }
}
