use serde::Serialize;
use std::path::PathBuf;
use tauri::{command, AppHandle, Manager};

use crate::media::thumbnail::{pick_tier, tier_file_name, BASE_TIER, HIGH_TIERS};

#[derive(Serialize)]
pub struct ThumbnailResult {
    pub id: String,
    pub path: String,
}

fn path_to_string(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// 在已知原图尺寸前先做一次无 DB 的快速查找：
/// 从高到低检查「≥ requested 且文件已存在」的档位，最后兜底基础档。
fn fast_existing(thumbs_dir: &std::path::Path, id: &str, requested: u32) -> Option<PathBuf> {
    let mut tiers: Vec<u32> = HIGH_TIERS
        .iter()
        .copied()
        .filter(|t| *t >= requested)
        .collect();
    tiers.reverse(); // 高 -> 低
    for t in tiers {
        let p = thumbs_dir.join(tier_file_name(id, t));
        if p.exists() {
            return Some(p);
        }
    }
    let base = thumbs_dir.join(tier_file_name(id, BASE_TIER));
    if base.exists() {
        return Some(base);
    }
    None
}

/// 不超过 `max_tier` 的最高已存在档位（生成失败时的降级兜底）。
fn fallback_existing(thumbs_dir: &std::path::Path, id: &str, max_tier: u32) -> Option<PathBuf> {
    let mut tiers: Vec<u32> = HIGH_TIERS
        .iter()
        .copied()
        .chain(std::iter::once(BASE_TIER))
        .filter(|t| *t <= max_tier)
        .collect();
    tiers.sort_unstable_by(|a, b| b.cmp(a)); // 高 -> 低
    for t in tiers {
        let p = thumbs_dir.join(tier_file_name(id, t));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// 懒生成：为单个媒体确保 `tier` 档缩略图存在，返回最佳可用路径。
///
/// 图片：从 library/variants/source_path 解析出本地文件后一次性 decode，
/// 补齐所有缺失的可用档位；视频：按目标档位单独 ffmpeg 抽帧。
/// 生成失败时降级返回已有的较低档位；全部缺失才返回 Err。
fn ensure_thumbnail(
    app: &AppHandle,
    id: &str,
    requested: u32,
    thumbs_dir: &std::path::Path,
) -> Result<String, String> {
    let media_list =
        crate::db::media_get_batch(app, &[id.to_string()]).map_err(|e| e.to_string())?;
    let media = match media_list.into_iter().next() {
        Some(m) => m,
        None => {
            return fallback_existing(thumbs_dir, id, requested)
                .map(|p| path_to_string(&p))
                .ok_or_else(|| format!("Thumbnail not found for {}", id));
        }
    };

    let longest = media
        .width
        .unwrap_or(0)
        .max(media.height.unwrap_or(0))
        .max(0) as u32;
    let tier = pick_tier(requested, longest);

    let tier_path = thumbs_dir.join(tier_file_name(id, tier));
    if !tier_path.exists() {
        // 解析本地源文件并按类型生成（所有缺失档位一次性补齐）
        match crate::db::resolve_media_file(app, id) {
            Ok(source) => {
                let is_video = media.media_type.as_deref() == Some("video");
                let result = if is_video {
                    crate::media::video_thumbnail::generate_video_thumbnail_sized(
                        app,
                        id,
                        &source,
                        media.duration,
                        tier,
                    )
                    .map(|_| ())
                } else {
                    image::open(&source)
                        .map_err(|e| e.to_string())
                        .and_then(|img| {
                            crate::media::thumbnail::generate_thumbnails_in_dir(
                                thumbs_dir, id, &img,
                            )
                            .map_err(|e| e.to_string())
                        })
                };
                if let Err(e) = result {
                    eprintln!(
                        "[thumbnail] lazy generation failed for {} tier {}: {}",
                        id, tier, e
                    );
                }
            }
            Err(e) => {
                eprintln!("[thumbnail] source resolve failed for {}: {}", id, e);
            }
        }
    }

    if tier_path.exists() {
        return Ok(path_to_string(&tier_path));
    }
    fallback_existing(thumbs_dir, id, tier)
        .map(|p| path_to_string(&p))
        .ok_or_else(|| format!("Thumbnail not found for {}", id))
}

#[command]
pub async fn media_thumbnail(
    app: AppHandle,
    id: String,
    size: Option<u32>,
) -> Result<String, String> {
    let requested = size.unwrap_or(BASE_TIER);
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumbs_dir = app_dir.join("thumbnails");

    if let Some(p) = fast_existing(&thumbs_dir, &id, requested) {
        return Ok(path_to_string(&p));
    }

    // 慢路径：需要 decode / ffmpeg，放到阻塞线程池，避免卡 UI
    let res = tauri::async_runtime::spawn_blocking(move || {
        ensure_thumbnail(&app, &id, requested, &thumbs_dir)
    })
    .await
    .map_err(|e| format!("thumbnail task failed: {}", e))?;
    res
}

/// Batch thumbnail resolution — single IPC + single DB query instead of N.
///
/// `size` 为渲染所需像素（最长边），后端据此选档；缺失档位会按媒体类型
/// 懒生成（图片 decode + 多档补齐 / 视频 ffmpeg 抽帧），失败降级到低档位，
/// 完全无缩略图的条目被省略（前端回退基础档或占位图）。
#[command]
pub async fn media_thumbnail_batch(
    app: AppHandle,
    ids: Vec<String>,
    size: Option<u32>,
) -> Result<Vec<ThumbnailResult>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let requested = size.unwrap_or(BASE_TIER);
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumbs_dir = app_dir.join("thumbnails");

    // 快速路径：全部命中已存在档位时零开销返回
    let mut results = Vec::with_capacity(ids.len());
    let mut missing = Vec::new();
    for id in &ids {
        match fast_existing(&thumbs_dir, id, requested) {
            Some(p) => results.push(ThumbnailResult {
                id: id.clone(),
                path: path_to_string(&p),
            }),
            None => missing.push(id.clone()),
        }
    }
    if missing.is_empty() {
        return Ok(results);
    }

    // 慢路径：懒生成放到阻塞线程池
    let res = tauri::async_runtime::spawn_blocking(move || {
        for id in &missing {
            if let Ok(path) = ensure_thumbnail(&app, &id, requested, &thumbs_dir) {
                results.push(ThumbnailResult {
                    id: id.clone(),
                    path,
                });
            }
        }
        results
    })
    .await
    .map_err(|e| format!("thumbnail batch task failed: {}", e))?;
    Ok(res)
}
