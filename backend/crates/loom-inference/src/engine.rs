//! InferenceEngine (local model stub) and CloudClient trait.

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{CompletionRequest, CompletionResponse, GpuInfo, ModelBackend, StreamDelta};
use std::path::Path;

#[derive(Debug)]
pub struct InferenceEngine {
    _model_path: std::path::PathBuf,
    _n_gpu_layers: usize,
}

impl InferenceEngine {
    pub async fn load(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        tracing::info!(path = %model_path.display(), n_gpu_layers, "loading model");
        Ok(Self { _model_path: model_path.to_path_buf(), _n_gpu_layers: if n_gpu_layers > 0 { n_gpu_layers } else { 0 } })
    }

    pub fn load_blocking(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        tracing::info!(path = %model_path.display(), n_gpu_layers, "loading model (sync)");
        Ok(Self { _model_path: model_path.to_path_buf(), _n_gpu_layers: n_gpu_layers })
    }

    pub fn dummy() -> Self {
        Self { _model_path: std::path::PathBuf::new(), _n_gpu_layers: 0 }
    }

    pub async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        Ok(stub_complete(&req))
    }

    pub async fn complete_stream(&self, req: CompletionRequest, token_tx: tokio::sync::mpsc::Sender<String>) -> Result<()> {
        let resp = self.complete(req).await?;
        let _ = token_tx.send(resp.text).await;
        Ok(())
    }

    pub fn token_count(&self, text: &str) -> usize {
        text.chars().count() / 4
    }

    pub fn detect_gpu() -> GpuInfo {
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=name,memory.total", "--format=csv,noheader"]).output()
            && output.status.success()
        {
            let info = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = info.lines().next() {
                let parts: Vec<&str> = line.split(',').collect();
                let vendor = parts.first().map(|s| s.trim().to_string()).unwrap_or_default();
                let vram_mb = parts.get(1)
                    .and_then(|s| s.trim().strip_suffix(" MiB"))
                    .and_then(|s| s.parse().ok()).unwrap_or(0);
                return GpuInfo { vendor, vram_mb, supported: vram_mb >= 4096 };
            }
        }
        GpuInfo { vendor: "none".into(), vram_mb: 0, supported: false }
    }
}

fn stub_complete(req: &CompletionRequest) -> CompletionResponse {
    let prompt_chars = req.prompt.chars().count();
    let response = "[openLoom] No inference backend available.\n\nConfigure a model in config.toml.".to_string();
    let response_tokens = response.chars().count() / 4;
    CompletionResponse {
        text: response, prompt_tokens: prompt_chars / 4, completion_tokens: response_tokens,
        cached_tokens: 0, latency_ms: 0, tool_calls: Vec::new(), thinking: None,
    }
}

#[async_trait]
pub trait CloudClient: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;
    async fn complete_stream(&self, req: CompletionRequest, tx: tokio::sync::mpsc::Sender<String>) -> Result<()>;
    async fn complete_stream_structured(&self, req: CompletionRequest, tx: tokio::sync::mpsc::Sender<StreamDelta>) -> Result<()> {
        let (legacy_tx, mut legacy_rx) = tokio::sync::mpsc::channel::<String>(256);
        let forwarder = tokio::spawn(async move {
            while let Some(token) = legacy_rx.recv().await {
                let delta = if let Some(stripped) = token.strip_prefix("\x02REASONING\x02") {
                    StreamDelta::Reasoning(stripped.to_string())
                } else if let Some(stripped) = token.strip_prefix("\x00USAGE:") {
                    let parts: Vec<&str> = stripped.split(':').collect();
                    if parts.len() == 3 {
                        StreamDelta::Usage { prompt_tokens: parts[0].parse().unwrap_or(0), completion_tokens: parts[1].parse().unwrap_or(0), cached_tokens: parts[2].parse().unwrap_or(0) }
                    } else { continue; }
                } else { StreamDelta::Text(token) };
                if tx.send(delta).await.is_err() { break; }
            }
        });
        self.complete_stream(req, legacy_tx).await?;
        forwarder.await.ok();
        Ok(())
    }
    fn provider(&self) -> ModelBackend;
    fn model_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_gpu_does_not_panic() {
        let info = InferenceEngine::detect_gpu();
        assert!(!info.vendor.is_empty() || !info.supported);
    }

    #[test]
    fn test_load_blocking_does_not_crash() {
        let result = InferenceEngine::load_blocking(std::path::Path::new("nonexistent.gguf"), 0);
        assert!(result.is_ok());
    }
}
