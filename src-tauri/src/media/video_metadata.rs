use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolve ffmpeg executable path.
/// Checks: 1) next to current exe, 2) binaries/ dir (dev mode), 3) PATH, 4) fallback.
pub fn find_ffmpeg() -> PathBuf {
    let names: &[&str] = if cfg!(windows) {
        &["ffmpeg.exe", "ffmpeg-x86_64-pc-windows-msvc.exe"]
    } else {
        &["ffmpeg"]
    };

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for name in names {
                let p = exe_dir.join(name);
                if is_valid_binary(&p) { return p; }
            }
            // In dev mode, walk up from target/debug/ to find binaries/
            let mut search = exe_dir.to_path_buf();
            for _ in 0..4 {
                if let Some(parent) = search.parent() {
                    search = parent.to_path_buf();
                }
                for name in names {
                    let p = search.join("binaries").join(name);
                    if is_valid_binary(&p) { return p; }
                }
            }
        }
    }
    if let Ok(path) = which::which("ffmpeg") {
        return path;
    }
    PathBuf::from("ffmpeg")
}

/// Check that a binary file is valid (exists and is larger than 1MB — skip empty placeholders).
fn is_valid_binary(path: &Path) -> bool {
    path.exists() && std::fs::metadata(path).map(|m| m.len() > 1024 * 1024).unwrap_or(false)
}

/// Resolve ffprobe executable path.
fn find_ffprobe() -> PathBuf {
    let names: &[&str] = if cfg!(windows) {
        &["ffprobe.exe", "ffprobe-x86_64-pc-windows-msvc.exe"]
    } else {
        &["ffprobe"]
    };

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for name in names {
                let p = exe_dir.join(name);
                if is_valid_binary(&p) { return p; }
            }
            let mut search = exe_dir.to_path_buf();
            for _ in 0..4 {
                if let Some(parent) = search.parent() {
                    search = parent.to_path_buf();
                }
                for name in names {
                    let p = search.join("binaries").join(name);
                    if is_valid_binary(&p) { return p; }
                }
            }
        }
    }
    if let Ok(path) = which::which("ffprobe") {
        return path;
    }
    PathBuf::from("ffprobe")
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: Option<FfprobeFormat>,
    streams: Option<Vec<FfprobeStream>>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    duration: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub width: i32,
    pub height: i32,
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub video_fps: Option<f64>,
}

/// Extract video metadata using ffprobe.
/// Finds the first video stream (does NOT assume streams[0] is video).
pub fn extract_metadata(input: &Path) -> Result<VideoMetadata, String> {
    let output = Command::new(find_ffprobe())
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(input)
        .output()
        .map_err(|e| format!("ffprobe execution failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed: {}", stderr));
    }

    let meta: FfprobeOutput =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("ffprobe JSON parse: {}", e))?;

    let video_stream = meta
        .streams
        .as_ref()
        .and_then(|streams| streams.iter().find(|s| s.codec_type.as_deref() == Some("video")))
        .ok_or("No video stream found in file")?;

    let width = video_stream.width.unwrap_or(0);
    let height = video_stream.height.unwrap_or(0);

    let duration = meta
        .format
        .as_ref()
        .and_then(|f| f.duration.as_ref())
        .and_then(|d| d.parse::<f64>().ok())
        .or_else(|| {
            video_stream
                .duration
                .as_ref()
                .and_then(|d| d.parse::<f64>().ok())
        });

    let video_codec = video_stream.codec_name.clone();

    let video_fps = video_stream
        .avg_frame_rate
        .as_ref()
        .and_then(|r| parse_fraction(r))
        .or_else(|| video_stream.r_frame_rate.as_ref().and_then(|r| parse_fraction(r)));

    Ok(VideoMetadata {
        width,
        height,
        duration,
        video_codec,
        video_fps,
    })
}

/// Quick check: does this file have a video stream?
pub fn has_video_stream(input: &Path) -> Result<bool, String> {
    let output = Command::new(find_ffprobe())
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_streams",
        ])
        .arg(input)
        .output()
        .map_err(|e| format!("ffprobe execution failed: {}", e))?;

    let meta: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("ffprobe JSON parse: {}", e))?;

    let has_video = meta["streams"]
        .as_array()
        .map(|streams| streams.iter().any(|s| s["codec_type"] == "video"))
        .unwrap_or(false);

    Ok(has_video)
}

fn parse_fraction(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num = parts[0].parse::<f64>().ok()?;
        let den = parts[1].parse::<f64>().ok()?;
        if den != 0.0 {
            return Some(num / den);
        }
    }
    s.parse::<f64>().ok()
}

/// Supported video extensions for initial screening
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mkv", "avi", "mov"];

/// Extract N evenly-spaced frames from a video using ffmpeg.
/// Frames are written as JPEGs to the system temp directory.
/// Returns paths to the extracted frame files.
pub fn extract_frames(
    video_path: &std::path::Path,
    duration_secs: f64,
    n_frames: u32,
) -> Result<Vec<PathBuf>, String> {
    if n_frames == 0 || duration_secs <= 0.0 {
        return Ok(Vec::new());
    }

    let n = n_frames.min(8);
    let interval = duration_secs / (n + 1) as f64;
    let temp_dir = std::env::temp_dir();
    let mut frames = Vec::new();

    for i in 1..=n {
        let timestamp = interval * i as f64;
        let frame_path = temp_dir.join(format!(
            "medix_video_frame_{}_{}.jpg",
            video_path.file_stem().unwrap_or_default().to_string_lossy(),
            i
        ));

        let result = Command::new(find_ffmpeg())
            .args([
                "-ss", &format!("{:.3}", timestamp),
                "-i",
            ])
            .arg(video_path)
            .args([
                "-frames:v", "1",
                "-q:v", "2",
                "-y",
            ])
            .arg(&frame_path)
            .output();

        match result {
            Ok(output) if output.status.success() && frame_path.exists() => {
                frames.push(frame_path);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "[video_ai] frame {}/{} at t={:.3}s failed: {}",
                    i, n, timestamp,
                    stderr.lines().last().unwrap_or("unknown error")
                );
            }
            Err(e) => {
                eprintln!("[video_ai] frame {}/{} ffmpeg error: {}", i, n, e);
            }
        }
    }

    Ok(frames)
}

/// Clean up extracted frame files
pub fn cleanup_frames(frames: &[PathBuf]) {
    for path in frames {
        let _ = std::fs::remove_file(path);
    }
}
