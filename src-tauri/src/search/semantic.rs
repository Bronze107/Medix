use std::collections::HashMap;
use tauri::AppHandle;

#[derive(Debug)]
pub struct ScoredMedia {
    pub media_id: String,
    pub score: f64,
}

/// Run semantic search given a pre-computed query embedding vector.
/// Returns scored media IDs sorted by cosine similarity descending.
pub fn semantic_search_by_vector(
    query_vec: &[f32],
    app: &AppHandle,
    limit: usize,
    min_score: f64,
) -> Result<Vec<ScoredMedia>, String> {
    let model = crate::settings::get_llama_model(app);
    if model.is_empty() {
        return Err("no model configured".to_string());
    }
    let model_short = std::path::Path::new(&model)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&model);

    let all_embs = crate::db::embedding_get_all_by_model(app, model_short)
        .map_err(|e| e.to_string())?;

    // Group by media_id: (caption_vec, tags_vec)
    let mut per_media: HashMap<String, (Option<Vec<f32>>, Option<Vec<f32>>)> = HashMap::new();
    for (media_id, content_type, vec) in all_embs {
        let entry = per_media.entry(media_id).or_insert((None, None));
        match content_type.as_str() {
            "caption" => entry.0 = Some(vec),
            "tags" => entry.1 = Some(vec),
            _ => {}
        }
    }

    let mut scored: Vec<ScoredMedia> = per_media
        .into_iter()
        .filter_map(|(media_id, (caption_emb, tags_emb))| {
            let caption_score = caption_emb
                .as_ref()
                .map(|v| cosine_similarity(query_vec, v))
                .unwrap_or(0.0);
            let tags_score = tags_emb
                .as_ref()
                .map(|v| cosine_similarity(query_vec, v))
                .unwrap_or(0.0);
            let combined = caption_score.max(tags_score);
            if combined > min_score {
                Some(ScoredMedia {
                    media_id,
                    score: combined,
                })
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);

    Ok(scored)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)) as f64
}
