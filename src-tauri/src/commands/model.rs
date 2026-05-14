use tauri::{command, AppHandle, Manager};

use crate::ai::server::LlamaServerStatus;
use crate::models;

#[command]
pub async fn llama_server_status(
    app: AppHandle,
) -> LlamaServerStatus {
    let server = app.state::<crate::ai::LlamaServer>();
    let port = crate::settings::get_llama_port(&app);
    server.status(port)
}

#[command]
pub async fn llama_server_start(app: AppHandle) -> Result<(), String> {
    let server = app.state::<crate::ai::LlamaServer>();
    let bin = crate::settings::get_llama_bin_path(&app);
    let model = crate::settings::get_llama_model(&app);
    let mmproj = crate::settings::get_llama_mmproj(&app);
    let port = crate::settings::get_llama_port(&app);
    let ctx = crate::settings::get_llama_ctx_size(&app);
    let threads = crate::settings::get_llama_threads(&app);
    let gpu = crate::settings::get_llama_gpu_layers(&app);
    server.start(&bin, &model, &mmproj, port, ctx, threads, gpu)?;
    server.wait_until_ready(port).await
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
