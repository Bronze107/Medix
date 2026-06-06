use serde::Serialize;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingServerStatus {
    pub running: bool,
    pub port: u16,
    pub pid: Option<u32>,
}

pub struct EmbeddingServer {
    process: Mutex<Option<Child>>,
}

impl EmbeddingServer {
    pub fn new() -> Self {
        Self {
            process: Mutex::new(None),
        }
    }

    pub fn start(
        &self,
        bin_path: &str,
        model_path: &str,
        port: u16,
        threads: u32,
    ) -> Result<(), String> {
        let mut guard = self
            .process
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;

        if guard.is_some() {
            if let Some(ref mut child) = *guard {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        *guard = None;
                    }
                    Ok(None) => return Ok(()),
                    Err(e) => return Err(format!("error checking child process: {}", e)),
                }
            }
        }

        if !std::path::Path::new(bin_path).exists() {
            return Err(format!("llama-server binary not found: {}", bin_path));
        }

        if model_path.is_empty() {
            return Err("no embedding model selected".to_string());
        }

        let model_full = if std::path::Path::new(model_path).is_absolute() {
            model_path.to_string()
        } else {
            model_path.to_string()
        };

        let mut cmd = Command::new(bin_path);
        cmd.arg("-m").arg(&model_full);
        cmd.arg("--host").arg("127.0.0.1");
        cmd.arg("--port").arg(port.to_string());
        cmd.arg("--threads").arg(threads.to_string());
        cmd.arg("--ctx-size").arg("512");
        cmd.arg("--n-gpu-layers").arg("0");
        cmd.arg("--embeddings");
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| format!("failed to spawn embedding server: {}", e))?;

        *guard = Some(child);
        Ok(())
    }

    pub async fn wait_until_ready(&self, port: u16) -> Result<(), String> {
        let url = format!("http://127.0.0.1:{}/health", port);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| e.to_string())?;

        for _ in 0..60 {
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return Ok(());
                }
                _ => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
        Err("embedding server did not become ready within 30 seconds".to_string())
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self
            .process
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;

        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        *guard = None;
        Ok(())
    }

    pub fn status(&self, port: u16) -> EmbeddingServerStatus {
        let guard = match self.process.lock() {
            Ok(g) => g,
            Err(_) => {
                return EmbeddingServerStatus {
                    running: false,
                    port,
                    pid: None,
                }
            }
        };
        if let Some(ref child) = *guard {
            EmbeddingServerStatus {
                running: true,
                port,
                pid: Some(child.id()),
            }
        } else {
            EmbeddingServerStatus {
                running: false,
                port,
                pid: None,
            }
        }
    }

    pub async fn health_check(&self, port: u16) -> bool {
        let url = format!("http://127.0.0.1:{}/health", port);
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
        {
            Ok(c) => c,
            Err(_) => return false,
        };
        match client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}
