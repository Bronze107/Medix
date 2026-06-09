use crate::db;
use crate::media::{BrowseItem, VariantVisibility};
use tauri::{command, AppHandle, Manager};

#[command]
pub async fn browse_list(
    app: AppHandle,
    sort_by: String,
    descending: bool,
    offset: u32,
    limit: u32,
    variant_visibility: String,
) -> Result<Vec<BrowseItem>, String> {
    let visibility = VariantVisibility::parse(&variant_visibility);
    db::list_browse_items(&app, &sort_by, descending, offset, limit, &visibility)
        .map_err(|e| e.to_string())
}

#[command]
pub async fn browse_search(
    app: AppHandle,
    query: String,
    sort_by: String,
    descending: bool,
    offset: u32,
    limit: u32,
    variant_visibility: String,
) -> Result<Vec<BrowseItem>, String> {
    let trimmed = query.trim().to_string();
    let visibility = VariantVisibility::parse(&variant_visibility);

    // Empty query falls back to browse_list
    if trimmed.is_empty() {
        return db::list_browse_items(&app, &sort_by, descending, offset, limit, &visibility)
            .map_err(|e| e.to_string());
    }

    // Run existing search pipeline to get candidate media IDs
    let parsed = crate::search::parser::parse(&trimmed);
    let query_embedding: Option<Vec<f32>> = if parsed.semantic_text.is_some() {
        let emb_model = crate::settings::get_embedding_model(&app);
        if emb_model.is_empty() {
            None
        } else {
            let emb_port = crate::settings::get_embedding_port(&app);
            let server = app.state::<crate::ai::EmbeddingServer>();
            if server.health_check(emb_port).await {
                match crate::ai::llamacpp::embed_text(
                    parsed.semantic_text.as_ref().unwrap(),
                    &emb_model,
                    emb_port,
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
        }
    } else {
        None
    };

    let app_clone = app.clone();
    let sort_clone = sort_by.clone();
    let min_score = crate::settings::get_semantic_threshold(&app);
    let search_result = tokio::task::spawn_blocking(move || {
        crate::search::execute_search(
            &app_clone,
            &trimmed,
            query_embedding,
            &sort_clone,
            descending,
            min_score,
        )
    })
    .await
    .map_err(|e| e.to_string())?;

    let media = search_result?;

    // Extract media IDs and expand to browse items
    let media_ids: Vec<String> = media.iter().map(|m| m.id.clone()).collect();
    db::browse_query_filtered(&app, &media_ids, &sort_by, descending, offset, limit, &visibility)
        .map_err(|e| e.to_string())
}

#[command]
pub fn browse_list_by_collection(
    app: AppHandle,
    collection_id: String,
    sort_by: String,
    descending: bool,
    offset: u32,
    limit: u32,
    variant_visibility: String,
) -> Result<Vec<BrowseItem>, String> {
    let visibility = VariantVisibility::parse(&variant_visibility);
    let media_ids = db::collection_get_item_ids(&app, &collection_id)
        .map_err(|e| e.to_string())?;
    if media_ids.is_empty() {
        return Ok(vec![]);
    }
    db::browse_query_filtered(&app, &media_ids, &sort_by, descending, offset, limit, &visibility)
        .map_err(|e| e.to_string())
}
