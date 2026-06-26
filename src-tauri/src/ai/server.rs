use serde::Serialize;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
pub struct LlamaServerStatus {
    pub running: bool,
    pub port: u16,
    pub pid: Option<u32>,
}

pub struct LlamaServer {
    process: Mutex<Option<Child>>,
}

impl LlamaServer {
    pub fn new() -> Self {
        Self {
            process: Mutex::new(None),
        }
    }

    pub fn start(
        &self,
        bin_path: &str,
        model_path: &str,
        mmproj_path: &str,
        port: u16,
        ctx_size: u32,
        threads: u32,
        gpu_layers: i32,
        cache_type_k: &str,
        cache_type_v: &str,
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

        self.start_locked(
            &mut guard,
            bin_path,
            model_path,
            mmproj_path,
            port,
            ctx_size,
            threads,
            gpu_layers,
            cache_type_k,
            cache_type_v,
        )
    }

    fn start_locked(
        &self,
        guard: &mut std::sync::MutexGuard<Option<Child>>,
        bin_path: &str,
        model_path: &str,
        mmproj_path: &str,
        port: u16,
        ctx_size: u32,
        threads: u32,
        gpu_layers: i32,
        cache_type_k: &str,
        cache_type_v: &str,
    ) -> Result<(), String> {

        if !std::path::Path::new(bin_path).exists() {
            return Err(format!("llama-server binary not found: {}", bin_path));
        }

        if model_path.is_empty() {
            return Err("no GGUF model selected".to_string());
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
        cmd.arg("--ctx-size").arg(ctx_size.to_string());
        cmd.arg("--threads").arg(threads.to_string());
        cmd.arg("--n-gpu-layers").arg(gpu_layers.to_string());
        if !mmproj_path.is_empty() {
            cmd.arg("--mmproj").arg(mmproj_path);
        }
        cmd.arg("--parallel").arg("2");
        cmd.arg("--reasoning").arg("off"); // MiniCPM-V Instruct: broken output with thinking
        cmd.arg("--flash-attn").arg("on"); // significant speedup for VLM inference
        cmd.arg("--cache-type-k").arg(cache_type_k);
        cmd.arg("--cache-type-v").arg(cache_type_v);
        cmd.arg("--mlock"); // keep model in RAM, prevent OS swap
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| format!("failed to spawn llama-server: {}", e))?;

        **guard = Some(child);
        Ok(())
    }

    /// Ensure the server is running, starting it if necessary.
    /// Idempotent and concurrency-safe: the lock serializes concurrent callers.
    pub async fn ensure_running(&self, app: &AppHandle) -> Result<(), String> {
        let port;
        {
            let mut guard = self
                .process
                .lock()
                .map_err(|e| format!("lock error: {}", e))?;

            // Already running — check if still alive
            if let Some(ref mut child) = *guard {
                match child.try_wait() {
                    Ok(None) => return Ok(()), // still running
                    Ok(Some(_)) => *guard = None, // exited, restart below
                    Err(e) => return Err(format!("error checking child process: {}", e)),
                }
            }

            // Not running — start it
            let bin = crate::settings::get_llama_bin_path(app);
            let model = crate::settings::get_llama_model(app);
            let mmproj = crate::settings::get_llama_mmproj(app);
            port = crate::settings::get_llama_port(app);
            let ctx = crate::settings::get_llama_ctx_size(app);
            let threads = crate::settings::get_llama_threads(app);
            let gpu = crate::settings::get_llama_gpu_layers(app);
            let cache_k = crate::settings::get_llama_cache_type_k(app);
            let cache_v = crate::settings::get_llama_cache_type_v(app);

            self.start_locked(
                &mut guard,
                &bin,
                &model,
                &mmproj,
                port,
                ctx,
                threads,
                gpu,
                &cache_k,
                &cache_v,
            )?;
        } // guard dropped here, before any await

        self.wait_until_ready(port).await
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

    pub fn status(&self, port: u16) -> LlamaServerStatus {
        let guard = match self.process.lock() {
            Ok(g) => g,
            Err(_) => {
                return LlamaServerStatus {
                    running: false,
                    port,
                    pid: None,
                }
            }
        };
        if let Some(ref child) = *guard {
            LlamaServerStatus {
                running: true,
                port,
                pid: Some(child.id()),
            }
        } else {
            LlamaServerStatus {
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

    pub fn is_port_available(&self, port: u16) -> bool {
        std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok()
    }
}
