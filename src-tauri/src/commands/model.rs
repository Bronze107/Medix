use tauri::command;

use crate::models;

#[command]
pub async fn ollama_status() -> Result<models::OllamaStatus, String> {
    Ok(models::check_ollama().await)
}

#[command]
pub fn model_list() -> Vec<models::ModelStatus> {
    models::model_status_list()
}
