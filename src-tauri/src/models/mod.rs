use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatus {
    pub name: String,
    pub installed: bool,
    pub size_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaStatus {
    pub running: bool,
    pub version: Option<String>,
    pub models: Vec<OllamaModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
    pub digest: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagModel>,
}

#[derive(Debug, Clone, Deserialize)]
struct OllamaTagModel {
    name: String,
    size: u64,
    digest: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OllamaVersionResponse {
    version: String,
}

const OLLAMA_BASE: &str = "http://127.0.0.1:11434";

pub async fn check_ollama() -> OllamaStatus {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return OllamaStatus {
                running: false,
                version: None,
                models: vec![],
            }
        }
    };

    let version = match client.get(format!("{}/api/version", OLLAMA_BASE)).send().await {
        Ok(resp) => match resp.json::<OllamaVersionResponse>().await {
            Ok(v) => Some(v.version),
            Err(_) => None,
        },
        Err(_) => {
            return OllamaStatus {
                running: false,
                version: None,
                models: vec![],
            }
        }
    };

    let models = match client.get(format!("{}/api/tags", OLLAMA_BASE)).send().await {
        Ok(resp) => match resp.json::<OllamaTagsResponse>().await {
            Ok(t) => t
                .models
                .into_iter()
                .map(|m| OllamaModel {
                    name: m.name,
                    size: m.size,
                    digest: m.digest,
                })
                .collect(),
            Err(_) => vec![],
        },
        Err(_) => vec![],
    };

    OllamaStatus {
        running: true,
        version,
        models,
    }
}

pub fn model_status_list() -> Vec<ModelStatus> {
    vec![
        ModelStatus {
            name: "MiniCPM-V 2.6".to_string(),
            installed: false,
            size_mb: 0.0,
        },
        ModelStatus {
            name: "nomic-embed-text".to_string(),
            installed: false,
            size_mb: 0.0,
        },
    ]
}
