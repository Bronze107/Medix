use tauri::{command, AppHandle};

use crate::media::{import, Media, MediaImportResult};
use crate::db;

#[command]
pub async fn media_import(app: AppHandle, paths: Vec<String>) -> Result<Vec<MediaImportResult>, String> {
    tokio::task::spawn_blocking(move || {
        import::import_files(&app, paths)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[command]
pub async fn media_list(
    app: AppHandle,
    sort_by: String,
    descending: bool,
) -> Result<Vec<Media>, String> {
    db::list_media(&app, &sort_by, descending)
        .map_err(|e| e.to_string())
}
