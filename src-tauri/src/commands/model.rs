use tauri::{command, AppHandle, Emitter, Manager};

use crate::ai::embedding::EmbeddingServerStatus;
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
    let cache_k = crate::settings::get_llama_cache_type_k(&app);
    let cache_v = crate::settings::get_llama_cache_type_v(&app);
    server.start(&bin, &model, &mmproj, port, ctx, threads, gpu, &cache_k, &cache_v)?;
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

#[command]
pub fn auto_detect(app: AppHandle) -> models::AutoDetect {
    models::auto_detect(&app)
}

#[command]
pub fn embedding_info(app: AppHandle, media_id: String, variant_id: Option<String>) -> Result<Vec<crate::db::EmbeddingInfo>, String> {
    crate::db::embedding_info_list(&app, &media_id, variant_id.as_deref()).map_err(|e| e.to_string())
}

#[command]
pub fn embedding_delete(app: AppHandle, media_id: String, variant_id: Option<String>) -> Result<(), String> {
    crate::db::embedding_delete_for_media(&app, &media_id, variant_id.as_deref()).map_err(|e| e.to_string())
}

#[command]
pub fn embedding_clear_all(app: AppHandle) -> Result<String, String> {
    let count = crate::db::embedding_clear_all(&app).map_err(|e| e.to_string())?;
    Ok(format!("已清除 {} 条 embedding", count))
}

#[command]
pub fn ai_pending_count(app: AppHandle) -> usize {
    let queue = app.state::<crate::ai::AiQueue>();
    queue.pending_count()
}

#[command]
pub async fn embedding_server_status(app: AppHandle) -> EmbeddingServerStatus {
    let server = app.state::<crate::ai::EmbeddingServer>();
    let port = crate::settings::get_embedding_port(&app);
    server.status(port)
}

#[derive(Clone, serde::Serialize)]
pub struct EmbeddingRebuildProgress {
    pub current: usize,
    pub total: usize,
}

#[command]
pub async fn embedding_rebuild_all(app: AppHandle) -> Result<String, String> {
    let emb_model = crate::settings::get_embedding_model(&app);
    if emb_model.is_empty() {
        return Err("未配置 embedding 模型".to_string());
    }

    let emb_port = crate::settings::get_embedding_port(&app);
    let emb_server = app.state::<crate::ai::EmbeddingServer>();
    if !emb_server.health_check(emb_port).await {
        return Err("embedding 服务器未运行".to_string());
    }

    // Get all active media (not deleted, non-variant)
    let all_media = crate::db::list_media(&app, "created_at", true, 0, u32::MAX)
        .map_err(|e| e.to_string())?;
    let total = all_media.len();

    let model_short = std::path::Path::new(&emb_model)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&emb_model)
        .to_string();

    for (i, media) in all_media.iter().enumerate() {
        // Get latest caption for this media
        let caption = crate::db::caption_list(&app, &media.id)
            .ok()
            .and_then(|list| list.into_iter().next())
            .map(|c| c.text)
            .unwrap_or_default();

        if caption.trim().is_empty() {
            continue;
        }

        match crate::ai::llamacpp::embed_text(&caption, &emb_model, emb_port).await {
            Ok(vector) => {
                if let Err(e) = crate::db::embedding_insert(
                    &app, &media.id, &model_short, "caption", None, &vector,
                ) {
                    eprintln!("[embedding-rebuild] failed to store caption embedding for {}: {}", media.id, e);
                } else {
                    println!("[embedding-rebuild] embedding stored for {} ({}d)", media.id, vector.len());
                }
            }
            Err(e) => {
                eprintln!("[embedding-rebuild] embedding failed for {}: {}", media.id, e);
            }
        }

        let _ = app.emit("embedding-rebuild-progress", EmbeddingRebuildProgress {
            current: i + 1,
            total,
        });
    }

    Ok(format!("重建完成，共 {} 条", total))
}
