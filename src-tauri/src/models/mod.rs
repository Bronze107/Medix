use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufModel {
    pub name: String,
    pub filename: String,
    pub path: String,
    pub size_mb: u64,
    pub is_vlm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufModelList {
    pub models: Vec<GgufModel>,
    pub models_dir: String,
}

fn models_dir(app: &AppHandle) -> PathBuf {
    let app_dir = app.path().app_data_dir().expect("Failed to get app data dir");
    app_dir.join("models")
}

pub fn get_gguf_models(app: &AppHandle) -> GgufModelList {
    let dir = models_dir(app);
    let _ = fs::create_dir_all(&dir);

    let mut models = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let name = filename
                .strip_suffix(".gguf")
                .unwrap_or(&filename)
                .to_string();
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let is_vlm = name.to_lowercase().contains("vlm")
                || name.to_lowercase().contains("vision")
                || name.to_lowercase().contains("vl")
                || name.to_lowercase().contains("minicpm")
                || name.to_lowercase().contains("multimodal");

            models.push(GgufModel {
                name,
                filename,
                path: path.to_string_lossy().to_string(),
                size_mb: size / (1024 * 1024),
                is_vlm,
            });
        }
    }

    models.sort_by(|a, b| a.filename.cmp(&b.filename));

    GgufModelList {
        models,
        models_dir: dir.to_string_lossy().to_string(),
    }
}

pub fn model_exists(app: &AppHandle, name_or_path: &str) -> bool {
    if name_or_path.is_empty() {
        return false;
    }
    let p = std::path::Path::new(name_or_path);
    if p.is_absolute() {
        p.exists()
    } else {
        let dir = models_dir(app);
        dir.join(name_or_path).exists()
            || dir.join(format!("{}.gguf", name_or_path)).exists()
    }
}
