mod ollama;

pub use ollama::{generate_caption, embed_text, AiResult};

use std::path::PathBuf;
use tauri::AppHandle;
use tokio::sync::mpsc;

pub enum AiTask {
    GenerateCaption {
        media_id: String,
        image_path: PathBuf,
    },
}

#[derive(Clone)]
pub struct AiQueue {
    sender: mpsc::Sender<AiTask>,
}

impl AiQueue {
    pub async fn send(&self,
        task: AiTask,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<AiTask>> {
        self.sender.send(task).await
    }
}

pub fn init_ai_queue(app: AppHandle) -> AiQueue {
    let (tx, mut rx) = mpsc::channel::<AiTask>(100);

    tokio::spawn(async move {
        while let Some(task) = rx.recv().await {
            match task {
                AiTask::GenerateCaption {
                    media_id,
                    image_path,
                } => {
                    if let Err(e) =
                        process_generate_caption(app.clone(), media_id, image_path).await
                    {
                        eprintln!("[ai] failed to process caption generation: {}", e);
                    }
                }
            }
        }
    });

    AiQueue { sender: tx }
}

async fn process_generate_caption(
    app: AppHandle,
    media_id: String,
    image_path: PathBuf,
) -> Result<(), String> {
    let ai_mode = crate::settings::get_ai_mode(&app);

    // If explicitly cloud, skip for now (cloud not implemented yet)
    if ai_mode == "cloud" {
        eprintln!("[ai] cloud mode not yet implemented, skipping {}", media_id);
        return Ok(());
    }

    // Check Ollama availability for local/auto mode
    let ollama = crate::models::check_ollama().await;
    if !ollama.running {
        if ai_mode == "local" {
            eprintln!("[ai] Ollama not running and mode is local, skipping {}", media_id);
        }
        return Ok(());
    }

    // Use minicpm-v for VLM caption
    let vlm_model = "minicpm-v";
    let embed_model = "nomic-embed-text";

    // Check if models are available
    let has_vlm = ollama.models.iter().any(|m| m.name.starts_with(vlm_model));
    let has_embed = ollama.models.iter().any(|m| m.name.starts_with(embed_model));

    if !has_vlm {
        eprintln!(
            "[ai] VLM model {} not found in Ollama, skipping {}",
            vlm_model, media_id
        );
        return Ok(());
    }

    println!("[ai] generating caption for {}...", media_id);

    let result = generate_caption(&image_path, vlm_model).await.map_err(|e| {
        eprintln!("[ai] caption generation failed for {}: {}", media_id, e);
        e.to_string()
    })?;

    println!(
        "[ai] caption generated for {}: {} ({} tags)",
        media_id,
        result.caption.chars().take(60).collect::<String>(),
        result.tags.len()
    );

    // Store caption with source='ai'
    crate::db::caption_create_with_source(
        &app,
        &media_id,
        &result.caption,
        Some("ai"),
    ).map_err(|e| e.to_string())?;

    // Store AI tags with source='ai' and confidence=0.9
    for tag_name in &result.tags {
        let tag_id = match ensure_tag(&app, tag_name) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("[ai] failed to ensure tag '{}': {}", tag_name, e);
                continue;
            }
        };
        if let Err(e) =
            crate::db::media_tag_add_with_source(&app, &media_id, &tag_id, Some(0.9), Some("ai"))
        {
            eprintln!("[ai] failed to add tag '{}': {}", tag_name, e);
        }
    }

    // Generate embeddings if embed model is available
    if has_embed {
        // Embed the caption
        match embed_text(&result.caption, embed_model).await {
            Ok(vector) => {
                if let Err(e) = crate::db::embedding_insert(
                    &app,
                    &media_id,
                    embed_model,
                    "caption",
                    &vector,
                ) {
                    eprintln!("[ai] failed to store caption embedding: {}", e);
                }
            }
            Err(e) => {
                eprintln!("[ai] caption embedding failed: {}", e);
            }
        }

        // Embed the tags (joined as comma-separated)
        if !result.tags.is_empty() {
            let tags_text = result.tags.join(", ");
            match embed_text(&tags_text, embed_model).await {
                Ok(vector) => {
                    if let Err(e) = crate::db::embedding_insert(
                        &app,
                        &media_id,
                        embed_model,
                        "tags",
                        &vector,
                    ) {
                        eprintln!("[ai] failed to store tags embedding: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("[ai] tags embedding failed: {}", e);
                }
            }
        }
    }

    println!("[ai] completed processing {}", media_id);
    Ok(())
}

fn ensure_tag(
    app: &AppHandle,
    name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let existing = crate::db::tag_list(app)?;
    let name_lower = name.to_lowercase();
    if let Some(tag) = existing.iter().find(|t| t.name == name_lower) {
        return Ok(tag.id.clone());
    }
    crate::db::tag_create(app, &name_lower)
}
