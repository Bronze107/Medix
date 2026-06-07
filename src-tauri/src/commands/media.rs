use tauri::{command, AppHandle, Emitter, Manager};

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
    offset: u32,
    limit: u32,
) -> Result<Vec<Media>, String> {
    db::list_media(&app, &sort_by, descending, offset, limit).map_err(|e| e.to_string())
}

#[command]
pub async fn media_search(
    app: AppHandle,
    query: String,
    sort_by: String,
    descending: bool,
    offset: u32,
    limit: u32,
) -> Result<Vec<Media>, String> {
    let trimmed = query.trim().to_string();

    // Quick path: empty query returns all (paginated)
    if trimmed.is_empty() {
        return db::list_media(&app, &sort_by, descending, offset, limit).map_err(|e| e.to_string());
    }

    // Parse query to check if semantic search is needed
    let parsed = crate::search::parser::parse(&trimmed);

    // If semantic text present, get embedding via dedicated embedding server
    let query_embedding: Option<Vec<f32>> = if parsed.semantic_text.is_some() {
        let emb_model = crate::settings::get_embedding_model(&app);
        if emb_model.is_empty() {
            eprintln!("[search] no embedding model configured, skipping semantic search");
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
                eprintln!("[search] embedding server not running, skipping semantic search");
                None
            }
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

#[derive(serde::Serialize)]
pub struct MediaPaths {
    pub original: Option<String>,
    pub thumb_256: Option<String>,
}

#[command]
pub fn media_get_paths(app: AppHandle, id: String) -> Result<MediaPaths, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    let original = {
        let library_dir = app_dir.join("library");
        let mut found = None;
        if library_dir.exists() {
            for entry in std::fs::read_dir(&library_dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with(&format!("{}.", &id)) {
                    found = Some(entry.path().to_string_lossy().replace('\\', "/"));
                    break;
                }
            }
        }
        found
    };

    let thumb_dir = app_dir.join("thumbnails");
    let thumb_256 = {
        let p = thumb_dir.join(format!("{}_256.jpg", &id));
        p.exists().then(|| p.to_string_lossy().replace('\\', "/"))
    };

    Ok(MediaPaths {
        original,
        thumb_256,
    })
}

#[command]
pub fn media_ai_annotate(app: AppHandle, id: String) -> Result<(), String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    // Find original file in library
    let library_dir = app_dir.join("library");
    let mut file_path = None;
    if library_dir.exists() {
        for entry in std::fs::read_dir(&library_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(&format!("{}.", &id)) {
                file_path = Some(entry.path());
                break;
            }
        }
    }

    let file_path = file_path.ok_or("Original file not found in library")?;

    // Check media type — video uses multi-frame VLM, image uses single-frame
    let media = crate::db::media_get_by_id(&app, &id)
        .map_err(|e| e.to_string())?
        .ok_or("Media not found in database")?;

    let queue = app.state::<crate::ai::AiQueue>();

    if media.media_type.as_deref() == Some("video") {
        let duration = media.duration.unwrap_or(0.0);
        queue
            .send(crate::ai::AiTask::GenerateVideoCaption {
                media_id: id.clone(),
                video_path: file_path,
                duration_secs: duration,
                variant_id: None,
            })
            .map_err(|e| e.to_string())?;
    } else {
        queue
            .send(crate::ai::AiTask::GenerateCaption {
                media_id: id.clone(),
                image_path: file_path,
                variant_id: None,
            })
            .map_err(|e| e.to_string())?;
    }

    // Notify frontend immediately
    let _ = app.emit(
        "ai-task-done",
        crate::ai::AiTaskProgress {
            remaining: queue.pending_count(),
        },
    );

    Ok(())
}
