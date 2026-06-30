use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;

use super::workflow::WorkflowManager;
use super::{EditParams, GenerateParams, GeneratedImage, ImageProvider, ImagineError};
use crate::db::comfyui::ComfyWorkflow;

pub struct ComfyuiProvider {
    base_url: String,
    timeout_secs: u64,
    workflow: ComfyWorkflow,
    client: reqwest::Client,
}

impl ComfyuiProvider {
    pub fn new(base_url: String, timeout_secs: u64, workflow: ComfyWorkflow) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build ComfyUI HTTP client");
        Self {
            base_url,
            timeout_secs,
            workflow,
            client,
        }
    }

    async fn submit_and_wait(
        &self,
        values: HashMap<String, String>,
    ) -> Result<Vec<GeneratedImage>, ImagineError> {
        let workflow_json = WorkflowManager::inject(&self.workflow.workflow_json, &values)
            .map_err(|e| ImagineError::Api(e))?;

        let workflow_value: Value = serde_json::from_str(&workflow_json)
            .map_err(|e| ImagineError::Api(format!("Invalid injected workflow: {}", e)))?;

        let body = serde_json::json!({
            "prompt": workflow_value,
            "client_id": "medix"
        });

        let submit_url = format!("{}/prompt", self.base_url);
        let resp = self
            .client
            .post(&submit_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    ImagineError::Api(format!("ComfyUI not running at {}", self.base_url))
                } else {
                    ImagineError::Http(e)
                }
            })?;

        let resp_json: Value = resp.json().await.map_err(ImagineError::Http)?;
        let prompt_id = resp_json["prompt_id"]
            .as_str()
            .ok_or(ImagineError::Api("No prompt_id in ComfyUI response".into()))?
            .to_string();

        let start = Instant::now();
        let history_url = format!("{}/history/{}", self.base_url, prompt_id);
        loop {
            if start.elapsed() > Duration::from_secs(self.timeout_secs) {
                return Err(ImagineError::Api(format!(
                    "Task timed out after {}s",
                    self.timeout_secs
                )));
            }

            let hist_resp = self
                .client
                .get(&history_url)
                .send()
                .await
                .map_err(ImagineError::Http)?;
            let hist_json: Value = hist_resp.json().await.map_err(ImagineError::Http)?;

            if let Some(outputs) = hist_json[&prompt_id]["outputs"].as_object() {
                let mut images = Vec::new();
                for (_node_id, node_output) in outputs {
                    if let Some(img_list) = node_output["images"].as_array() {
                        for img_info in img_list {
                            let filename = img_info["filename"].as_str().unwrap_or("");
                            let subfolder = img_info["subfolder"].as_str().unwrap_or("");
                            let img_type = img_info["type"].as_str().unwrap_or("output");

                            let dl_url = format!(
                                "{}/view?filename={}&subfolder={}&type={}",
                                self.base_url, filename, subfolder, img_type
                            );

                            let dl_resp = self
                                .client
                                .get(&dl_url)
                                .send()
                                .await
                                .map_err(ImagineError::Http)?;
                            let data = dl_resp.bytes().await.map_err(ImagineError::Http)?;

                            let mime_type =
                                if filename.ends_with(".png") { "image/png" } else { "image/jpeg" };

                            images.push(GeneratedImage {
                                mime_type: mime_type.to_string(),
                                data: data.to_vec(),
                            });
                        }
                    }
                }

                if images.is_empty() {
                    return Err(ImagineError::EmptyResponse);
                }
                return Ok(images);
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

#[async_trait]
impl ImageProvider for ComfyuiProvider {
    async fn generate(
        &self,
        params: &GenerateParams,
    ) -> Result<Vec<GeneratedImage>, ImagineError> {
        let mut values = HashMap::new();
        values.insert("prompt".to_string(), params.prompt.clone());
        self.submit_and_wait(values).await
    }

    async fn edit(&self, params: &EditParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        let upload_url = format!("{}/upload/image", self.base_url);

        let (mime, b64_data) = if let Some(comma) = params.image_data_url.find(',') {
            let data = &params.image_data_url[comma + 1..];
            let mime = if params.image_data_url.contains("image/png") {
                "image/png"
            } else {
                "image/jpeg"
            };
            (mime, data.to_string())
        } else {
            return Err(ImagineError::Api("Invalid image data URL".into()));
        };

        let img_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &b64_data,
        )
        .map_err(|e| ImagineError::Api(format!("Failed to decode base64: {}", e)))?;

        let part = reqwest::multipart::Part::bytes(img_bytes)
            .file_name(if mime == "image/png" {
                "input.png".to_string()
            } else {
                "input.jpg".to_string()
            })
            .mime_str(mime)
            .map_err(|e| ImagineError::Api(e.to_string()))?;

        let form = reqwest::multipart::Form::new().part("image", part);

        let upload_resp = self
            .client
            .post(&upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(ImagineError::Http)?;

        let upload_json: Value = upload_resp.json().await.map_err(ImagineError::Http)?;
        let uploaded_filename = upload_json["name"]
            .as_str()
            .ok_or(ImagineError::Api("No filename from upload response".into()))?;

        let mut values = HashMap::new();
        values.insert("prompt".to_string(), params.prompt.clone());
        values.insert("input_image".to_string(), uploaded_filename.to_string());

        self.submit_and_wait(values).await
    }

    async fn health_check(&self) -> Result<bool, ImagineError> {
        let url = format!("{}/system_stats", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}
