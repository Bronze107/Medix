use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use tauri::{command, AppHandle, Manager};

use crate::captions::Caption;
use crate::db;

// --- Per-scope generation counter ---
// When multiple caption operations target the same (media_id, variant_id) scope
// in quick succession, only the last-spawned embedding task actually runs;
// earlier tasks detect they've been superseded and skip their HTTP call.
type ScopeKey = (String, Option<String>); // (media_id, variant_id)

static EMBED_GENERATIONS: LazyLock<Mutex<HashMap<ScopeKey, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn next_generation(media_id: &str, variant_id: Option<&str>) -> (ScopeKey, u64) {
    let key = (media_id.to_string(), variant_id.map(|s| s.to_string()));
    let mut map = EMBED_GENERATIONS.lock().unwrap();
    let gen = map.entry(key.clone()).or_insert(0);
    *gen += 1;
    (key, *gen)
}

fn is_latest_generation(key: &ScopeKey, gen: u64) -> bool {
    EMBED_GENERATIONS.lock().unwrap().get(key).copied() == Some(gen)
}

// --- Core helpers ---

/// After a caption is created/updated/deleted, try to refresh the embedding
/// from the latest caption for semantic search. variant_id scopes to original (None) or variant.
/// When no captions remain, the stale embedding is deleted.
///
/// If `known_text` is provided, it is used directly instead of querying caption_list
/// (for create/update where the caller already has the text).
///
/// If `scope_gen` is provided, the task checks whether it has been superseded by a
/// newer task for the same scope before doing the HTTP call (and again before the DB insert).
async fn refresh_embedding(
    app: &AppHandle,
    media_id: &str,
    variant_id: Option<&str>,
    known_text: Option<&str>,
    scope_gen: Option<&(ScopeKey, u64)>,
) {
    let emb_model = crate::settings::get_embedding_model(app);
    if emb_model.is_empty() {
        return;
    }
    let emb_port = crate::settings::get_embedding_port(app);
    let emb_server = app.state::<crate::ai::EmbeddingServer>();
    if let Err(e) = emb_server.ensure_running(app).await {
        eprintln!("[caption] embedding server failed to start, skipping auto-embed for {}: {}", media_id, e);
        return;
    }

    // If a newer task was spawned for this scope, skip the HTTP call.
    if let Some((key, gen)) = scope_gen {
        if !is_latest_generation(key, *gen) {
            eprintln!("[caption] superseded, skipping embed for {}", media_id);
            return;
        }
    }

    let text = match known_text {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => {
            let caption = match db::caption_list(app, media_id) {
                Ok(list) => {
                    let filtered: Vec<_> = match variant_id {
                        Some(vid) => list.into_iter().filter(|c| c.variant_id.as_deref() == Some(vid)).collect(),
                        None => list.into_iter().filter(|c| c.variant_id.is_none()).collect(),
                    };
                    filtered.into_iter().next().map(|c| c.text)
                }
                Err(_) => None,
            };
            match caption {
                Some(t) if !t.trim().is_empty() => t,
                _ => {
                    let _ = db::embedding_delete_for_media(app, media_id, variant_id);
                    eprintln!("[caption] no caption to embed for {}, deleted stale embedding", media_id);
                    return;
                }
            }
        }
    };

    let model_short = std::path::Path::new(&emb_model)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&emb_model)
        .to_string();

    match crate::ai::llamacpp::embed_text(&text, &emb_model, emb_port).await {
        Ok(vector) => {
            // Re-check after the HTTP call — a newer task may have been spawned
            // while we were waiting.
            if let Some((key, gen)) = scope_gen {
                if !is_latest_generation(key, *gen) {
                    eprintln!("[caption] superseded after embed, discarding for {}", media_id);
                    return;
                }
            }
            if let Err(e) = db::embedding_insert(app, media_id, &model_short, "caption", variant_id, &vector) {
                eprintln!("[caption] failed to store caption embedding for {}: {}", media_id, e);
            } else {
                println!("[caption] embedding stored for {} ({}d)", media_id, vector.len());
            }
        }
        Err(e) => {
            eprintln!("[caption] embedding failed for {}: {}", media_id, e);
        }
    }
}

/// Check whether a caption is the latest for its (media_id, variant_id) scope.
fn is_latest_caption(list: &[Caption], caption_id: &str, variant_id: Option<&str>) -> bool {
    let filtered: Vec<_> = match variant_id {
        Some(vid) => list.iter().filter(|c| c.variant_id.as_deref() == Some(vid)).collect(),
        None => list.iter().filter(|c| c.variant_id.is_none()).collect(),
    };
    filtered.first().map(|c| c.id == caption_id).unwrap_or(false)
}

// --- Spawn helper ---

/// Spawn a refresh_embedding call in the background, deduplicated by scope.
/// If another task for the same (media_id, variant_id) is already in flight,
/// the new generation counter will cause the older task to skip its HTTP call.
fn spawn_embed(
    app: &AppHandle,
    media_id: &str,
    variant_id: Option<&str>,
    known_text: Option<&str>,
) {
    let gen = next_generation(media_id, variant_id);
    let app_h = app.clone();
    let mid = media_id.to_string();
    let vid = variant_id.map(|s| s.to_string());
    let txt = known_text.map(|s| s.to_string());
    tokio::spawn(async move {
        let key_ref = &gen.0;
        refresh_embedding(
            &app_h,
            &mid,
            vid.as_deref(),
            txt.as_deref(),
            Some(&(key_ref.clone(), gen.1)),
        ).await;
    });
}

// --- Tauri commands ---

#[command]
pub fn caption_list(app: AppHandle, media_id: String) -> Result<Vec<Caption>, String> {
    db::caption_list(&app, &media_id).map_err(|e| e.to_string())
}

#[command]
pub async fn caption_create(app: AppHandle, media_id: String, text: String) -> Result<Caption, String> {
    let caption = db::caption_create(&app, &media_id, &text).map_err(|e| e.to_string())?;
    spawn_embed(&app, &media_id, None, Some(&text));
    Ok(caption)
}

#[command]
pub async fn caption_create_for_variant(
    app: AppHandle,
    media_id: String,
    variant_id: String,
    text: String,
) -> Result<Caption, String> {
    let caption = db::caption_create_for_variant(&app, &media_id, &variant_id, &text, None)
        .map_err(|e| e.to_string())?;
    spawn_embed(&app, &media_id, Some(&variant_id), Some(&text));
    Ok(caption)
}

#[command]
pub async fn caption_update(app: AppHandle, id: String, text: String) -> Result<(), String> {
    let (media_id, variant_id) = db::caption_update(&app, &id, &text).map_err(|e| e.to_string())?;
    if media_id.is_empty() {
        return Ok(());
    }
    // Only refresh if the updated caption is the latest — otherwise the embedding hasn't changed.
    let is_latest = db::caption_list(&app, &media_id)
        .ok()
        .map(|list| is_latest_caption(&list, &id, variant_id.as_deref()))
        .unwrap_or(false);
    if is_latest {
        spawn_embed(&app, &media_id, variant_id.as_deref(), Some(&text));
    }
    Ok(())
}

#[command]
pub async fn caption_delete(app: AppHandle, id: String) -> Result<(), String> {
    let (media_id, variant_id) = db::caption_get_media_info(&app, &id).map_err(|e| e.to_string())?;
    if media_id.is_empty() {
        return Ok(());
    }
    let is_latest = db::caption_list(&app, &media_id)
        .ok()
        .map(|list| is_latest_caption(&list, &id, variant_id.as_deref()))
        .unwrap_or(false);
    db::caption_delete(&app, &id).map_err(|e| e.to_string())?;
    if is_latest {
        spawn_embed(&app, &media_id, variant_id.as_deref(), None);
    }
    Ok(())
}

#[command]
pub async fn caption_create_batch(
    app: AppHandle,
    media_ids: Vec<String>,
    text: String,
) -> Result<(), String> {
    if media_ids.is_empty() {
        return Ok(());
    }
    // DB work must be isolated from .await — rusqlite types are !Send
    {
        let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let db_path = app_data.join("medix.db");
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
        conn.execute("BEGIN TRANSACTION", []).map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("INSERT INTO captions (id, media_id, text, source) VALUES (?1, ?2, ?3, ?4)")
            .map_err(|e| e.to_string())?;
        let result = (|| -> Result<(), String> {
            for media_id in &media_ids {
                let id = ulid::Ulid::new().to_string();
                stmt.execute(rusqlite::params![id, media_id, text, "manual"])
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        drop(stmt);
        match result {
            Ok(()) => {
                conn.execute("COMMIT", []).map_err(|e| e.to_string())?;
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                return Err(e);
            }
        }
    }
    for mid in &media_ids {
        let _ = db::fts_sync(&app, mid);
    }
    // Batch embedding runs in background; no per-item dedup needed since
    // batch is rarely triggered interactively.
    let app_h = app.clone();
    let mids = media_ids.clone();
    let txt = text.clone();
    tokio::spawn(async move {
        embed_batch(&app_h, &mids, &txt).await;
    });
    Ok(())
}

/// Generate one embedding vector for `text` and insert it for all `media_ids`
/// in a single transaction. Avoids N sequential HTTP calls.
async fn embed_batch(app: &AppHandle, media_ids: &[String], text: &str) {
    let emb_model = crate::settings::get_embedding_model(app);
    if emb_model.is_empty() {
        return;
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let emb_port = crate::settings::get_embedding_port(app);
    let emb_server = app.state::<crate::ai::EmbeddingServer>();
    if let Err(e) = emb_server.ensure_running(app).await {
        eprintln!("[caption] embedding server failed to start, skipping batch embed: {}", e);
        return;
    }

    let model_short = std::path::Path::new(&emb_model)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&emb_model)
        .to_string();

    match crate::ai::llamacpp::embed_text(trimmed, &emb_model, emb_port).await {
        Ok(vector) => {
            let app_data = match app.path().app_data_dir() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[caption] batch embed: app_data_dir failed: {}", e);
                    return;
                }
            };
            let db_path = app_data.join("medix.db");
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[caption] batch embed: cannot open db: {}", e);
                    return;
                }
            };
            let blob: Vec<u8> = vector.iter().flat_map(|f| f.to_le_bytes()).collect();
            let result: Result<(), Box<dyn std::error::Error>> = (|| {
                conn.execute("BEGIN TRANSACTION", [])?;
                for mid in media_ids {
                    conn.execute(
                        "DELETE FROM embeddings WHERE media_id = ?1 AND model = ?2 AND content_type = 'caption' AND variant_id IS NULL",
                        rusqlite::params![mid, model_short],
                    )?;
                    conn.execute(
                        "INSERT INTO embeddings (media_id, model, content_type, variant_id, vector) VALUES (?1, ?2, 'caption', NULL, ?3)",
                        rusqlite::params![mid, model_short, blob],
                    )?;
                }
                conn.execute("COMMIT", [])?;
                Ok(())
            })();
            match result {
                Ok(()) => println!(
                    "[caption] batch embedding stored for {} items ({}d)",
                    media_ids.len(),
                    vector.len(),
                ),
                Err(e) => {
                    let _ = conn.execute("ROLLBACK", []);
                    eprintln!("[caption] batch embed failed: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("[caption] batch embedding failed: {}", e);
        }
    }
}
