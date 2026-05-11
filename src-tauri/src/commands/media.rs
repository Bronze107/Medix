use tauri::{command, AppHandle};

use crate::db::{self, TagSearchMode};
use crate::media::{import, Media, MediaImportResult};

#[command]
pub async fn media_import(
    app: AppHandle,
    paths: Vec<String>,
) -> Result<Vec<MediaImportResult>, String> {
    tokio::task::spawn_blocking(move || import::import_files(&app, paths).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[command]
pub async fn media_list(
    app: AppHandle,
    sort_by: String,
    descending: bool,
) -> Result<Vec<Media>, String> {
    db::list_media(&app, &sort_by, descending).map_err(|e| e.to_string())
}

#[command]
pub async fn media_search(
    app: AppHandle,
    query: String,
    sort_by: String,
    descending: bool,
) -> Result<Vec<Media>, String> {
    tokio::task::spawn_blocking(move || {
        let trimmed = query.trim();
        if trimmed.starts_with("tag:") {
            let tag_part = trimmed[4..].trim();
            let (tag_names, mode) = if tag_part.contains(" OR ") {
                let names: Vec<String> = tag_part
                    .split(" OR ")
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                (names, TagSearchMode::Union)
            } else if tag_part.contains('|') {
                let names: Vec<String> = tag_part
                    .split('|')
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                (names, TagSearchMode::Union)
            } else {
                let names: Vec<String> = tag_part
                    .split_whitespace()
                    .map(|s| s.to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                (names, TagSearchMode::Intersection)
            };
            db::media_search_by_tags(&app, &tag_names, &sort_by, descending, mode)
                .map_err(|e| e.to_string())
        } else {
            db::list_media(&app, &sort_by, descending).map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
