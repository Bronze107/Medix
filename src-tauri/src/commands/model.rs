use tauri::{command, AppHandle, Manager};

use crate::ai::server::LlamaServerStatus;
use crate::models;

#[command]
pub async fn llama_server_status(
    app: AppHandle,
) -> LlamaServerStatus {
    let server = app.state::<crate::ai::LlamaServer>();
    server.status()
}

#[command]
pub async fn llama_server_start(app: AppHandle) -> Result<(), String> {
    let server = app.state::<crate::ai::LlamaServer>();
    server.start()?;
    server.wait_until_ready().await
}

#[command]
pub async fn llama_server_stop(app: AppHandle) -> Result<(), String> {
    let server = app.state::<crate::ai::LlamaServer>();
    server.stop()
}

#[command]
pub fn model_list(app: AppHandle) -> models::GgufModelList {
    models::get_gguf_models(&app)
}
