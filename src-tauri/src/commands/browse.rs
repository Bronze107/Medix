use std::collections::HashMap;

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

/// Given a list of browse items and tag names, return the item_ids that directly have those tags.
/// Checks per-item: original items use variant_id=NULL, variant items use their variant_id.
fn filter_items_by_tags(
    app: &AppHandle,
    items: &mut Vec<BrowseItem>,
    tag_names: &[String],
) -> Result<(), String> {
    if tag_names.is_empty() || items.is_empty() {
        return Ok(());
    }
    let matching = db::find_items_with_tags(app, items, tag_names)
        .map_err(|e| e.to_string())?;
    items.retain(|it| matching.contains(&it.item_id));
    Ok(())
}

/// In representative mode, collapse items to at most one per media.
/// Prefer: display variant > tag-matching variant > tag-matching original > (none).
fn collapse_representative(items: &mut Vec<BrowseItem>) {
    // Group by media_id, pick best item per group
    let mut best: std::collections::HashMap<String, BrowseItem> = std::collections::HashMap::new();
    for it in items.drain(..) {
        let key = &it.media_id;
        let score = if it.is_display_variant { 3 }
            else if it.item_kind == "variant" { 2 }
            else { 1 };
        best.entry(key.clone())
            .and_modify(|existing| {
                let existing_score = if existing.is_display_variant { 3 }
                    else if existing.item_kind == "variant" { 2 }
                    else { 1 };
                if score > existing_score {
                    *existing = it.clone();
                }
            })
            .or_insert(it);
    }
    // Sort results by imported_at (descending) for consistency
    let mut sorted: Vec<BrowseItem> = best.into_values().collect();
    sorted.sort_by(|a, b| b.imported_at.cmp(&a.imported_at));
    *items = sorted;
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

    // Parse query to extract tag filters
    let parsed = crate::search::parser::parse(&trimmed);
    let tag_names: Vec<String> = parsed.tag_group.as_ref()
        .map(|tg| tg.tags.clone())
        .unwrap_or_default();
    let has_tag_filter = !tag_names.is_empty();

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
    let query_emb_for_items = query_embedding.clone();
    let search_result = tokio::task::spawn_blocking(move || {
        let media = crate::search::execute_search(
            &app_clone,
            &trimmed,
            query_embedding,
            &sort_clone,
            descending,
            min_score,
        )?;
        // Compute item-level semantic scores: key = (media_id, variant_id)
        let item_semantic_scores: Option<HashMap<(String, Option<String>), f64>> =
            query_emb_for_items.and_then(|vec| {
                match crate::search::semantic::semantic_search_by_vector(&vec, &app_clone, 500, min_score) {
                    Ok(scored) => {
                        let mut map = HashMap::new();
                        for s in scored {
                            map.insert((s.media_id, s.variant_id), s.score);
                        }
                        Some(map)
                    }
                    Err(e) => {
                        eprintln!("[search] item-level semantic failed: {}", e);
                        None
                    }
                }
            });
        Ok::<_, String>((media, item_semantic_scores))
    })
    .await
    .map_err(|e| e.to_string())?;

    let (media, item_semantic_scores) = search_result?;
    let media_ids: Vec<String> = media.iter().map(|m| m.id.clone()).collect();

    // Always expand in "all" mode to get full set, then filter
    let mut items = db::browse_query_filtered(
        &app, &media_ids, &sort_by, descending, 0, u32::MAX, &VariantVisibility::All,
    ).map_err(|e| e.to_string())?;

    // Item-level semantic ranking: sort by own embedding score,
    // drop items whose score is far below their media group's top scorer.
    if let Some(ref scores) = item_semantic_scores {
        eprintln!("[search] item-level scores map has {} entries", scores.len());
        let item_score: HashMap<String, f64> = items.iter().map(|it| {
            let key = (it.media_id.clone(), it.variant_id.clone());
            let s = scores.get(&key).copied().unwrap_or(0.0);
            eprintln!("[search]   item={} media={} vid={:?} score={:.4}",
                &it.item_id[..8.min(it.item_id.len())], &it.media_id[..8], it.variant_id.as_deref().map(|v| &v[..8]), s);
            (it.item_id.clone(), s)
        }).collect();
        let mut group_max: HashMap<String, f64> = HashMap::new();
        for it in items.iter() {
            let s = item_score[&it.item_id];
            let e = group_max.entry(it.media_id.clone()).or_insert(0.0);
            *e = (*e).max(s);
        }
        items.retain(|it| {
            let s = item_score.get(&it.item_id).copied().unwrap_or(0.0);
            let max = group_max.get(&it.media_id).copied().unwrap_or(0.0);
            let keep = max == 0.0 || s >= max * 0.5;
            if !keep {
                eprintln!("[search]   DROP item={} (score={:.4} < max*0.5={:.4})",
                    &it.item_id[..8.min(it.item_id.len())], s, max * 0.5);
            }
            keep
        });
        items.sort_by(|a, b| {
            let sa = item_score.get(&a.item_id).copied().unwrap_or(0.0);
            let sb = item_score.get(&b.item_id).copied().unwrap_or(0.0);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Step 1: item-level tag filter — only keep items that directly have the searched tags
    if has_tag_filter {
        filter_items_by_tags(&app, &mut items, &tag_names)?;
    }

    // Step 2: apply visibility
    match visibility {
        VariantVisibility::All => {
            // Keep all matching items (already filtered)
        }
        VariantVisibility::Representative => {
            collapse_representative(&mut items);
        }
    }

    // Apply pagination
    let total = items.len();
    let start = offset as usize;
    let end = std::cmp::min(start + limit as usize, total);
    if start >= total {
        return Ok(vec![]);
    }
    Ok(items[start..end].to_vec())
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
