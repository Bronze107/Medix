use tauri::{command, AppHandle, Manager};

use crate::db;
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
    let trimmed = query.trim().to_string();

    // Quick path: empty query returns all
    if trimmed.is_empty() {
        return db::list_media(&app, &sort_by, descending).map_err(|e| e.to_string());
    }

    // Parse query to check if semantic search is needed
    let parsed = crate::search::parser::parse(&trimmed);

    // If semantic text present, get embedding (async, outside spawn_blocking)
    let query_embedding: Option<Vec<f32>> = if parsed.semantic_text.is_some() {
        let model = crate::settings::get_llama_model(&app);
        let port = crate::settings::get_llama_port(&app);
        if !model.is_empty() {
            let server = app.state::<crate::ai::LlamaServer>();
            if server.health_check(port).await {
                match crate::ai::llamacpp::embed_text(
                    parsed.semantic_text.as_ref().unwrap(),
                    &model,
                    port,
                )
                .await
                {
                    Ok(vec) => Some(vec),
                    Err(e) => {
                        eprintln!("[search] embedding failed: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Run the search engine in spawn_blocking (DB + CPU work)
    let app_clone = app.clone();
    let min_score = crate::settings::get_semantic_threshold(&app);
    tokio::task::spawn_blocking(move || {
        crate::search::execute_search(
            &app_clone,
            &trimmed,
            query_embedding,
            &sort_by,
            descending,
            min_score,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[command]
pub fn media_soft_delete(app: AppHandle, id: String) -> Result<(), String> {
    db::media_soft_delete(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn media_recover(app: AppHandle, id: String) -> Result<(), String> {
    db::media_recover(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn media_permanent_delete(app: AppHandle, id: String) -> Result<(), String> {
    db::media_permanent_delete(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub fn media_list_trash(
    app: AppHandle,
    sort_by: String,
    descending: bool,
) -> Result<Vec<Media>, String> {
    db::media_list_trash(&app, &sort_by, descending).map_err(|e| e.to_string())
}

#[command]
pub fn media_empty_trash(app: AppHandle) -> Result<usize, String> {
    db::media_empty_trash(&app).map_err(|e| e.to_string())
}

#[command]
pub fn media_find_duplicates(app: AppHandle) -> Result<Vec<Vec<Media>>, String> {
    db::media_find_similar(&app, 10).map_err(|e| e.to_string())
}
