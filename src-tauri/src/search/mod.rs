pub mod parser;
pub mod semantic;

use crate::db::TagSearchMode;
use crate::media::Media;
use crate::settings;
use std::collections::{HashMap, HashSet};
use tauri::AppHandle;

use parser::TagMatchMode;

/// Main search entry point. Called from the media_search Tauri command.
pub fn execute_search(
    app: &AppHandle,
    query: &str,
    query_embedding: Option<Vec<f32>>,
    sort_by: &str,
    descending: bool,
    min_score: f64,
) -> Result<Vec<Media>, String> {
    let parsed = parser::parse(query);

    // Step 1: Semantic search
    let semantic_map: Option<HashMap<String, f64>> =
        query_embedding
            .as_ref()
            .and_then(|vec| match semantic::semantic_search_by_vector(vec, app, 500, min_score) {
                Ok(results) => {
                    Some(results.into_iter().map(|r| (r.media_id, r.score)).collect())
                }
                Err(e) => {
                    eprintln!("[search] semantic search failed: {}", e);
                    None
                }
            });

    // Step 1b: FTS5 full-text search (runs alongside semantic, results merged)
    let fts_ids: Option<HashSet<String>> = if crate::settings::is_fts5_search_enabled(app) {
        parsed.semantic_text.as_ref().and_then(|text| {
            match crate::db::fts_search(app, text, 500) {
                Ok(ids) if !ids.is_empty() => Some(ids.into_iter().collect()),
                Ok(_) => None,
                Err(e) => {
                    eprintln!("[search] fts5 search failed: {}", e);
                    None
                }
            }
        })
    } else {
        None
    };

    // Step 2: Tag filter
    let tag_ids: Option<HashSet<String>> = parsed
        .tag_group
        .as_ref()
        .map(|tg| {
            let mode = match tg.mode {
                TagMatchMode::All => TagSearchMode::Intersection,
                TagMatchMode::Any => TagSearchMode::Union,
            };
            crate::db::media_search_by_tags(app, &tg.tags, sort_by, descending, mode)
                .map(|list| list.into_iter().map(|m| m.id).collect())
        })
        .transpose()
        .map_err(|e| e.to_string())?;

    // Step 3: Combine candidates (intersection of semantic + FTS + tag results)
    let candidate_ids: Option<Vec<String>> = {
        // Merge semantic and FTS results
        let text_ids: Option<HashSet<String>> = match (&semantic_map, &fts_ids) {
            (Some(sem), Some(fts)) => {
                let mut merged: HashSet<String> = sem.keys().cloned().collect();
                merged.extend(fts.iter().cloned());
                Some(merged)
            }
            (Some(sem), None) => Some(sem.keys().cloned().collect()),
            (None, Some(fts)) => Some(fts.clone()),
            (None, None) => None,
        };
        // Intersect with tag filter
        match (&text_ids, &tag_ids) {
            (Some(txt), Some(tag)) => {
                let ids: Vec<String> = txt.iter().filter(|id| tag.contains(*id)).cloned().collect();
                if ids.is_empty() {
                    return Ok(vec![]);
                }
                Some(ids)
            }
            (Some(txt), None) => Some(txt.iter().cloned().collect()),
            (None, Some(tag)) => Some(tag.iter().cloned().collect()),
            (None, None) => None,
        }
    };

    // Step 4: If no filters at all, return all media
    if candidate_ids.is_none()
        && parsed.dimensions.is_empty()
        && parsed.date_range.is_none()
        && parsed.file_size.is_none()
        && parsed.media_type.is_none()
    {
        return crate::db::list_media(app, sort_by, descending, 0, u32::MAX).map_err(|e| e.to_string());
    }

    // Step 5: Apply metadata filters via SQL
    let mut results = crate::db::media_query_filtered(
        app,
        candidate_ids.as_deref(),
        &parsed.dimensions,
        &parsed.date_range,
        &parsed.file_size,
        &parsed.media_type,
        sort_by,
        descending,
    )
    .map_err(|e| e.to_string())?;

    // Step 6: If semantic, sort by score; otherwise keep DB sort order
    if let Some(ref sem_map) = semantic_map {
        results.sort_by(|a, b| {
            let sa = sem_map.get(&a.id).copied().unwrap_or(0.0);
            let sb = sem_map.get(&b.id).copied().unwrap_or(0.0);
            sb.partial_cmp(&sa)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Step 7: Resolve thumbnail paths
    crate::db::resolve_thumb_paths(app, &mut results);

    Ok(results)
}

/// Standalone search entry point for CLI / testing (no Tauri AppHandle).
/// Does NOT perform semantic search (requires embedding DB) or thumbnail path resolution.
pub fn execute_search_path(
    db_path: &std::path::Path,
    query: &str,
    sort_by: &str,
    descending: bool,
    fuzzy_tags: bool,
) -> Result<Vec<Media>, String> {
    let parsed = parser::parse(query);

    // Step 2: Tag filter
    let tag_ids: Option<HashSet<String>> = parsed
        .tag_group
        .as_ref()
        .map(|tg| {
            let mode = match tg.mode {
                TagMatchMode::All => TagSearchMode::Intersection,
                TagMatchMode::Any => TagSearchMode::Union,
            };
            crate::db::media_search_by_tags_path(db_path, &tg.tags, sort_by, descending, mode, fuzzy_tags)
                .map(|list| list.into_iter().map(|m| m.id).collect())
        })
        .transpose()
        .map_err(|e| e.to_string())?;

    // Step 3: Combine candidates (no semantic in CLI mode)
    let candidate_ids: Option<Vec<String>> = tag_ids.map(|t| t.iter().cloned().collect());

    // Step 4: If no filters at all, return all media (unless pure semantic text — can't search without embedding)
    if candidate_ids.is_none()
        && parsed.dimensions.is_empty()
        && parsed.date_range.is_none()
        && parsed.file_size.is_none()
        && parsed.media_type.is_none()
    {
        if parsed.semantic_text.is_some() {
            // Semantic query without embedding server — return empty rather than all
            return Ok(vec![]);
        }
        return crate::db::list_media_path(db_path, sort_by, descending, 0, u32::MAX)
            .map_err(|e| e.to_string());
    }

    // Step 5: Apply metadata filters via SQL
    crate::db::media_query_filtered_path(
        db_path,
        candidate_ids.as_deref(),
        &parsed.dimensions,
        &parsed.date_range,
        &parsed.file_size,
        &parsed.media_type,
        sort_by,
        descending,
    )
    .map_err(|e| e.to_string())
}
