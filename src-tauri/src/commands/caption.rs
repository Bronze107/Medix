use tauri::{command, AppHandle, Manager};

use crate::captions::Caption;
use crate::db;

/// After a caption is created/updated/deleted, try to refresh the embedding
/// from the latest caption for semantic search. variant_id scopes to original (None) or variant.
/// When no captions remain, the stale embedding is deleted.
async fn refresh_embedding(app: &AppHandle, media_id: &str, variant_id: Option<&str>) {
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
    let text = match caption {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            // No more captions — clean up stale embedding
            let _ = db::embedding_delete_for_media(app, media_id, variant_id);
            eprintln!("[caption] no caption to embed for {}, deleted stale embedding", media_id);
            return;
        }
    };

    let model_short = std::path::Path::new(&emb_model)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&emb_model)
        .to_string();

    match crate::ai::llamacpp::embed_text(&text, &emb_model, emb_port).await {
        Ok(vector) => {
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

#[command]
pub fn caption_list(app: AppHandle, media_id: String) -> Result<Vec<Caption>, String> {
    db::caption_list(&app, &media_id).map_err(|e| e.to_string())
}

#[command]
pub async fn caption_create(app: AppHandle, media_id: String, text: String) -> Result<Caption, String> {
    let caption = db::caption_create(&app, &media_id, &text).map_err(|e| e.to_string())?;
    refresh_embedding(&app, &media_id, None).await;
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
    refresh_embedding(&app, &media_id, Some(&variant_id)).await;
    Ok(caption)
}

#[command]
pub async fn caption_update(app: AppHandle, id: String, text: String) -> Result<(), String> {
    let (media_id, variant_id) = {
        let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let db_path = app_data.join("medix.db");
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT media_id, variant_id FROM captions WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .ok()
        .unwrap_or_default()
    };
    db::caption_update(&app, &id, &text).map_err(|e| e.to_string())?;
    if !media_id.is_empty() {
        refresh_embedding(&app, &media_id, variant_id.as_deref()).await;
    }
    Ok(())
}

#[command]
pub async fn caption_delete(app: AppHandle, id: String) -> Result<(), String> {
    let (media_id, variant_id) = {
        let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let db_path = app_data.join("medix.db");
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT media_id, variant_id FROM captions WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .ok()
        .unwrap_or_default()
    };
    db::caption_delete(&app, &id).map_err(|e| e.to_string())?;
    if !media_id.is_empty() {
        refresh_embedding(&app, &media_id, variant_id.as_deref()).await;
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
    // Now safe to .await — all rusqlite handles are dropped
    for mid in &media_ids {
        let _ = db::fts_sync(&app, mid);
        refresh_embedding(&app, mid, None).await;
    }
    Ok(())
}
