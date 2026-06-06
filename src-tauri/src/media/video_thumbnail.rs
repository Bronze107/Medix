use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

/// Generate a 256px thumbnail from a video file using ffmpeg.
/// Tries multiple timestamps in order: 10% of duration -> 1.0s -> 50% -> first frame.
pub fn generate_video_thumbnail(
    app: &AppHandle,
    media_id: &str,
    source_path: &Path,
    duration_secs: Option<f64>,
) -> Result<PathBuf, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumbs_dir = app_dir.join("thumbnails");
    std::fs::create_dir_all(&thumbs_dir).map_err(|e| e.to_string())?;
    let thumb_path = thumbs_dir.join(format!("{}_256.jpg", media_id));

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

    for (i, ts) in timestamps.iter().enumerate() {
        let result = Command::new(crate::media::video_metadata::find_ffmpeg())
            .args([
                "-ss", ts,
                "-i",
            ])
            .arg(source_path)
            .args([
                "-frames:v", "1",
                "-vf", "scale=256:256:force_original_aspect_ratio=decrease,pad=256:256:(ow-iw)/2:(oh-ih)/2",
                "-q:v", "3",
                "-y",
            ])
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
