use tauri::{command, AppHandle};

use crate::export::ExportOptions;

#[command]
pub async fn export_dataset(
    app: AppHandle,
    options: ExportOptions,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || crate::export::run_export(&app, &options))
        .await
        .map_err(|e| e.to_string())?
}

#[command]
pub async fn import_zip(app: AppHandle, zip_path: String) -> Result<usize, String> {
    tokio::task::spawn_blocking(move || crate::export::import_zip(&app, &zip_path))
        .await
        .map_err(|e| e.to_string())?
}
