use serde::Serialize;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct LlamaServerStatus {
    pub running: bool,
    pub port: u16,
    pub pid: Option<u32>,
}

pub struct LlamaServer {
    process: Mutex<Option<Child>>,
    port: u16,
    bin_path: String,
    model_path: String,
    ctx_size: u32,
    threads: u32,
    gpu_layers: i32,
}

impl LlamaServer {
    pub fn new(
        port: u16,
        bin_path: &str,
        model_path: &str,
        ctx_size: u32,
        threads: u32,
        gpu_layers: i32,
    ) -> Self {
        Self {
            process: Mutex::new(None),
            port,
            bin_path: bin_path.to_string(),
            model_path: model_path.to_string(),
            ctx_size,
            threads,
            gpu_layers,
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let mut guard = self
            .process
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;

        if guard.is_some() {
            // Already running — check if still alive
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

        let bin = &self.bin_path;
        if !std::path::Path::new(bin).exists() {
            return Err(format!("llama-server binary not found: {}", bin));
        }

        if self.model_path.is_empty() {
            return Err("no GGUF model selected".to_string());
        }

        let model_full = if std::path::Path::new(&self.model_path).is_absolute() {
            self.model_path.clone()
        } else {
            // Relative path: resolve against app data models dir?
            // For now treat as-is
            self.model_path.clone()
        };

        let ctx = self.ctx_size.to_string();
        let threads = self.threads.to_string();
        let gpu = self.gpu_layers.to_string();
        let port = self.port.to_string();

        let mut cmd = Command::new(bin);
        cmd.args([
            "-m", &model_full,
            "--host", "127.0.0.1",
            "--port", &port,
            "--ctx-size", &ctx,
            "--threads", &threads,
            "--n-gpu-layers", &gpu,
        ]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| format!("failed to spawn llama-server: {}", e))?;

        *guard = Some(child);
        Ok(())
    }

    pub async fn wait_until_ready(&self) -> Result<(), String> {
        let url = format!("http://127.0.0.1:{}/health", self.port);
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
        Err("llama-server did not become ready within 30 seconds".to_string())
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

    pub fn status(&self) -> LlamaServerStatus {
        let guard = match self.process.lock() {
            Ok(g) => g,
            Err(_) => {
                return LlamaServerStatus {
                    running: false,
                    port: self.port,
                    pid: None,
                }
            }
        };
        if let Some(ref child) = *guard {
            LlamaServerStatus {
                running: true,
                port: self.port,
                pid: Some(child.id()),
            }
        } else {
            LlamaServerStatus {
                running: false,
                port: self.port,
                pid: None,
            }
        }
    }

    pub async fn health_check(&self) -> bool {
        let url = format!("http://127.0.0.1:{}/health", self.port);
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

    pub fn is_port_available(&self) -> bool {
        std::net::TcpListener::bind(format!("127.0.0.1:{}", self.port)).is_ok()
    }
}
