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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoDetect {
    pub binary_paths: Vec<String>,
    pub binary_path: String,
    pub mmproj_files: Vec<String>,
}

/// Auto-detect llama-server binary from PATH and common install locations
pub fn auto_detect(app: &AppHandle) -> AutoDetect {
    // Scan for mmproj files
    let dir = models_dir(app);
    let mut mmproj_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            if (name.contains("mmproj") || name.contains("clip") || name.contains("vision"))
                && path.extension().and_then(|e| e.to_str()) == Some("gguf")
            {
                mmproj_files.push(path.to_string_lossy().to_string());
            }
        }
    }

    // Detect binary
    let candidates = [
        "llama-server.exe",
        "llama-server",
    ];
    let mut binary_paths = Vec::new();
    let mut binary_path = String::new();

    for name in &candidates {
        // Check in dirs next to common install locations
        let search_dirs = [
            "C:\\llama-vulkan",
            "C:\\llama-cpp",
            "C:\\llama.cpp",
            &format!("{}\\llama-vulkan", std::env::var("USERPROFILE").unwrap_or_default()),
            &format!("{}\\llama-cpp", std::env::var("USERPROFILE").unwrap_or_default()),
        ];
        for dir in &search_dirs {
            let p = std::path::Path::new(dir).join(name);
            if p.exists() {
                let path_str = p.to_string_lossy().to_string();
                if binary_path.is_empty() {
                    binary_path = path_str.clone();
                }
                if !binary_paths.contains(&path_str) {
                    binary_paths.push(path_str);
                }
            }
            // Also check bin/ subdirectory
            let p = std::path::Path::new(dir).join("bin").join(name);
            if p.exists() {
                let path_str = p.to_string_lossy().to_string();
                if binary_path.is_empty() {
                    binary_path = path_str.clone();
                }
                if !binary_paths.contains(&path_str) {
                    binary_paths.push(path_str);
                }
            }
        }
    }

    // Check PATH
    for name in &candidates {
        if let Ok(path) = which::which(name) {
            let path_str = path.to_string_lossy().to_string();
            if binary_path.is_empty() {
                binary_path = path_str.clone();
            }
            if !binary_paths.contains(&path_str) {
                binary_paths.push(path_str);
            }
        }
    }

    AutoDetect {
        binary_paths,
        binary_path,
        mmproj_files,
    }
}
