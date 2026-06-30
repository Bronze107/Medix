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
        eprintln!(
            "[comfyui] provider created: base_url={}, timeout={}s, workflow={} ({})",
            base_url, timeout_secs, workflow.name, workflow.id
        );
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
        eprintln!(
            "[comfyui] submit_and_wait start: values={:?}, workflow_name={}",
            values, self.workflow.name
        );

        let t_total = Instant::now();

        // 1. Inject params into workflow JSON
        let t_inject = Instant::now();
        eprintln!(
            "[comfyui] raw workflow_json (first 300 chars): {}",
            &self.workflow.workflow_json.chars().take(300).collect::<String>()
        );
        let workflow_json = WorkflowManager::inject(&self.workflow.workflow_json, &values)
            .map_err(|e| {
                eprintln!("[comfyui] inject FAILED: {}", e);
                ImagineError::Api(e)
            })?;
        eprintln!(
            "[comfyui] inject duration_ms={}, result (first 500 chars): {}",
            t_inject.elapsed().as_millis(),
            &workflow_json.chars().take(500).collect::<String>()
        );

        let workflow_value: Value = serde_json::from_str(&workflow_json)
            .map_err(|e| {
                eprintln!("[comfyui] parse injected json FAILED: {}", e);
                ImagineError::Api(format!("Invalid injected workflow: {}", e))
            })?;

        let body = serde_json::json!({
            "prompt": workflow_value,
            "client_id": "medix"
        });

        // 2. POST /prompt
        let submit_url = format!("{}/prompt", self.base_url);
        eprintln!(
            "[comfyui] POST {} body_size={} bytes",
            submit_url,
            serde_json::to_string(&body).unwrap_or_default().len()
        );
        let t_post = Instant::now();
        let resp = self
            .client
            .post(&submit_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    eprintln!("[comfyui] POST connect FAILED: {}", e);
                    ImagineError::Api(format!("ComfyUI not running at {}", self.base_url))
                } else {
                    eprintln!("[comfyui] POST FAILED (non-connect): {} kind={:?}", e, e.status());
                    ImagineError::Http(e)
                }
            })?;
        eprintln!(
            "[comfyui] POST response: HTTP {} duration_ms={}",
            resp.status(),
            t_post.elapsed().as_millis()
        );

        let t_parse = Instant::now();
        let resp_text = resp.text().await.map_err(ImagineError::Http)?;
        eprintln!(
            "[comfyui] response body (first 500 chars): {}",
            &resp_text.chars().take(500).collect::<String>()
        );
        let resp_json: Value = serde_json::from_str(&resp_text).map_err(|e| {
            eprintln!(
                "[comfyui] parse response FAILED: {} body={}",
                e,
                &resp_text.chars().take(200).collect::<String>()
            );
            ImagineError::Api(format!("Failed to parse ComfyUI response: {}", e))
        })?;
        eprintln!(
            "[comfyui] response parse duration_ms={}",
            t_parse.elapsed().as_millis()
        );

        // Check for error in response
        if let Some(err) = resp_json["error"].as_str() {
            eprintln!("[comfyui] server returned error: {}", err);
            let node_errors = resp_json["node_errors"]
                .as_object()
                .map(|o| format!("{:?}", o))
                .unwrap_or_default();
            return Err(ImagineError::Api(format!(
                "ComfyUI error: {} node_errors={}",
                err, node_errors
            )));
        }

        let prompt_id = resp_json["prompt_id"]
            .as_str()
            .ok_or_else(|| {
                eprintln!("[comfyui] no prompt_id in response: {:?}", resp_json);
                ImagineError::Api("No prompt_id in ComfyUI response".into())
            })?
            .to_string();

        eprintln!("[comfyui] got prompt_id={}", prompt_id);

        // 3. Poll GET /history/{prompt_id}
        let start = Instant::now();
        let history_url = format!("{}/history/{}", self.base_url, prompt_id);
        let mut poll_count = 0;
        loop {
            let elapsed = start.elapsed();
            if elapsed > Duration::from_secs(self.timeout_secs) {
                eprintln!(
                    "[comfyui] TIMEOUT after {}s ({} polls, limit {}s)",
                    elapsed.as_secs(),
                    poll_count,
                    self.timeout_secs
                );
                return Err(ImagineError::Api(format!(
                    "Task timed out after {}s",
                    self.timeout_secs
                )));
            }

            poll_count += 1;
            let t_poll = Instant::now();
            let hist_resp = self
                .client
                .get(&history_url)
                .send()
                .await
                .map_err(ImagineError::Http)?;
            let hist_status = hist_resp.status();
            let hist_text = hist_resp.text().await.map_err(ImagineError::Http)?;
            eprintln!(
                "[comfyui] poll #{}, HTTP {}, body_len={} duration_ms={}",
                poll_count,
                hist_status.as_u16(),
                hist_text.len(),
                t_poll.elapsed().as_millis()
            );

            let hist_json: Value = match serde_json::from_str(&hist_text) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!(
                        "[comfyui] poll #{} parse FAILED: {} body={}",
                        poll_count,
                        e,
                        &hist_text.chars().take(300).collect::<String>()
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            // Check for completion: history[prompt_id].outputs exists
            if let Some(outputs) = hist_json[&prompt_id]["outputs"].as_object() {
                eprintln!(
                    "[comfyui] DONE: poll #{} duration_ms={}, outputs keys={:?}",
                    poll_count,
                    start.elapsed().as_millis(),
                    outputs.keys().collect::<Vec<_>>()
                );

                let mut images = Vec::new();
                for (node_id, node_output) in outputs {
                    if let Some(img_list) = node_output["images"].as_array() {
                        eprintln!(
                            "[comfyui] node {} has {} image(s)",
                            node_id,
                            img_list.len()
                        );
                        for (i, img_info) in img_list.iter().enumerate() {
                            let filename = img_info["filename"].as_str().unwrap_or("");
                            let subfolder = img_info["subfolder"].as_str().unwrap_or("");
                            let img_type = img_info["type"].as_str().unwrap_or("output");

                            let dl_url = format!(
                                "{}/view?filename={}&subfolder={}&type={}",
                                self.base_url, filename, subfolder, img_type
                            );
                            eprintln!(
                                "[comfyui] downloading image {}/{}: {}",
                                i + 1,
                                img_list.len(),
                                dl_url
                            );

                            let t_dl = Instant::now();
                            let dl_resp = self
                                .client
                                .get(&dl_url)
                                .send()
                                .await
                                .map_err(ImagineError::Http)?;
                            let data = dl_resp.bytes().await.map_err(ImagineError::Http)?;
                            eprintln!(
                                "[comfyui] downloaded {} bytes duration_ms={}",
                                data.len(),
                                t_dl.elapsed().as_millis()
                            );

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
                    eprintln!(
                        "[comfyui] DONE but NO IMAGES in outputs: {:?}",
                        serde_json::to_string(&outputs).unwrap_or_default()
                    );
                    return Err(ImagineError::EmptyResponse);
                }
                eprintln!(
                    "[comfyui] total duration_ms={}, images={}",
                    t_total.elapsed().as_millis(),
                    images.len()
                );
                return Ok(images);
            }

            // Check for error/exception in history
            if let Some(status_obj) = hist_json[&prompt_id]["status"].as_object() {
                if let Some(status_str) = status_obj["status_str"].as_str() {
                    if status_str == "error" {
                        let messages = status_obj["messages"].as_array()
                            .map(|a| a.iter().filter_map(|m| m.as_str()).collect::<Vec<_>>().join("; "))
                            .unwrap_or_default();
                        eprintln!("[comfyui] task FAILED: status=error messages={}", messages);
                        return Err(ImagineError::Api(format!(
                            "ComfyUI execution failed: {}", messages
                        )));
                    }
                }
            }

            eprintln!("[comfyui] poll #{}: still running, sleeping 2s", poll_count);
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
        eprintln!(
            "[comfyui] generate: prompt={}, aspect_ratio={}, resolution={}, n={}",
            params.prompt, params.aspect_ratio, params.resolution, params.n
        );
        let mut values = HashMap::new();
        values.insert("prompt".to_string(), params.prompt.clone());
        let result = self.submit_and_wait(values).await;
        match &result {
            Ok(images) => eprintln!("[comfyui] generate SUCCESS: {} images", images.len()),
            Err(e) => eprintln!("[comfyui] generate FAILED: {}", e),
        }
        result
    }

    async fn edit(&self, params: &EditParams) -> Result<Vec<GeneratedImage>, ImagineError> {
        eprintln!(
            "[comfyui] edit: prompt={}, aspect_ratio={}, resolution={}, n={}, image_data_url_len={}",
            params.prompt,
            params.aspect_ratio,
            params.resolution,
            params.n,
            params.image_data_url.len()
        );

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
            eprintln!("[comfyui] edit: invalid image data URL");
            return Err(ImagineError::Api("Invalid image data URL".into()));
        };

        let img_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &b64_data,
        )
        .map_err(|e| {
            eprintln!("[comfyui] edit: base64 decode FAILED: {}", e);
            ImagineError::Api(format!("Failed to decode base64: {}", e))
        })?;
        eprintln!("[comfyui] edit: decoded {} bytes", img_bytes.len());

        let part = reqwest::multipart::Part::bytes(img_bytes)
            .file_name(if mime == "image/png" {
                "input.png".to_string()
            } else {
                "input.jpg".to_string()
            })
            .mime_str(mime)
            .map_err(|e| {
                eprintln!("[comfyui] edit: multipart part FAILED: {}", e);
                ImagineError::Api(e.to_string())
            })?;

        let form = reqwest::multipart::Form::new().part("image", part);

        eprintln!("[comfyui] edit: POST {}", upload_url);
        let t_upload = Instant::now();
        let upload_resp = self
            .client
            .post(&upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(ImagineError::Http)?;
        eprintln!(
            "[comfyui] edit: upload response HTTP {} duration_ms={}",
            upload_resp.status(),
            t_upload.elapsed().as_millis()
        );

        let upload_text = upload_resp.text().await.map_err(ImagineError::Http)?;
        eprintln!(
            "[comfyui] edit: upload body: {}",
            &upload_text.chars().take(300).collect::<String>()
        );
        let upload_json: Value = serde_json::from_str(&upload_text).map_err(|e| {
            eprintln!("[comfyui] edit: upload parse FAILED: {}", e);
            ImagineError::Api(format!("Failed to parse upload response: {}", e))
        })?;
        let uploaded_filename = upload_json["name"]
            .as_str()
            .ok_or_else(|| {
                eprintln!("[comfyui] edit: no filename in upload response: {:?}", upload_json);
                ImagineError::Api("No filename from upload response".into())
            })?;
        eprintln!(
            "[comfyui] edit: uploaded as filename={}",
            uploaded_filename
        );

        let mut values = HashMap::new();
        values.insert("prompt".to_string(), params.prompt.clone());
        values.insert("input_image".to_string(), uploaded_filename.to_string());

        let result = self.submit_and_wait(values).await;
        match &result {
            Ok(images) => eprintln!("[comfyui] edit SUCCESS: {} images", images.len()),
            Err(e) => eprintln!("[comfyui] edit FAILED: {}", e),
        }
        result
    }

    async fn health_check(&self) -> Result<bool, ImagineError> {
        let url = format!("{}/system_stats", self.base_url);
        eprintln!("[comfyui] health_check: GET {}", url);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                let ok = resp.status().is_success();
                eprintln!("[comfyui] health_check: {}", ok);
                Ok(ok)
            }
            Err(e) => {
                eprintln!("[comfyui] health_check FAILED: {}", e);
                Ok(false)
            }
        }
    }
}
