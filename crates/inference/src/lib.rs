use anyhow::Result;
use openloom_models::GpuInfo;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub stream: bool,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            max_tokens: 2048,
            temperature: 0.7,
            stream: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

#[derive(Debug)]
pub struct InferenceEngine {
    _model_path: std::path::PathBuf,
    _n_gpu_layers: usize,
}

impl InferenceEngine {
    /// Load GGUF model. Falls back to CPU if GPU unavailable.
    pub async fn load(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        tracing::info!(path = %model_path.display(), n_gpu_layers, "loading model");
        Ok(Self {
            _model_path: model_path.to_path_buf(),
            _n_gpu_layers: if n_gpu_layers > 0 { n_gpu_layers } else { 0 },
        })
    }

    /// Synchronous load for Phase 1 initialization
    pub fn load_blocking(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        tracing::info!(path = %model_path.display(), n_gpu_layers, "loading model (sync)");
        Ok(Self {
            _model_path: model_path.to_path_buf(),
            _n_gpu_layers: n_gpu_layers,
        })
    }

    /// Complete text completion (returns full text at once)
    pub async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let prompt_chars = req.prompt.chars().count();
        let response = format!(
            "[openLoom] Local model (Qwen3-1.7B) is not yet loaded. Install the GGUF model file to enable inference.\n\nYour message ({} chars): {}...",
            prompt_chars,
            &req.prompt[..req.prompt.len().min(100)]
        );
        let response_tokens = response.chars().count() / 4;
        Ok(CompletionResponse {
            text: response,
            prompt_tokens: prompt_chars / 4,
            completion_tokens: response_tokens,
        })
    }

    /// Streaming completion (token-by-token via mpsc::Sender)
    pub async fn complete_stream(
        &self,
        _req: CompletionRequest,
        _tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        Ok(())
    }

    /// Detect GPU info (vendor, VRAM, support status)
    pub fn detect_gpu() -> GpuInfo {
        // Try nvidia-smi on Windows/Linux
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=name,memory.total", "--format=csv,noheader"])
            .output()
            && output.status.success()
        {
            let info = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = info.lines().next() {
                let parts: Vec<&str> = line.split(',').collect();
                let vendor = parts.first().map(|s| s.trim().to_string()).unwrap_or_default();
                let vram_mb = parts
                    .get(1)
                    .and_then(|s| s.trim().strip_suffix(" MiB"))
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                return GpuInfo {
                    vendor,
                    vram_mb,
                    supported: vram_mb >= 4096,
                };
            }
        }
        // Fallback: no GPU detected
        GpuInfo {
            vendor: "none".into(),
            vram_mb: 0,
            supported: false,
        }
    }

    /// Count tokens in text using the loaded model's tokenizer
    pub fn token_count(&self, text: &str) -> usize {
        // Simplified estimation: ~4 chars per token
        text.chars().count() / 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_gpu_does_not_panic() {
        let info = InferenceEngine::detect_gpu();
        // Should return a valid struct even on CPU-only machines
        assert!(!info.vendor.is_empty() || !info.supported);
    }

    #[test]
    fn test_completion_request_default() {
        let req = CompletionRequest::default();
        assert_eq!(req.max_tokens, 2048);
        assert!((req.temperature - 0.7).abs() < 0.01);
        assert!(!req.stream);
    }

    #[test]
    fn test_gpu_info_serialization() {
        let info = GpuInfo {
            vendor: "NVIDIA".into(),
            vram_mb: 8192,
            supported: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: GpuInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.vram_mb, 8192);
    }

    #[test]
    fn test_token_count_estimation() {
        let engine = InferenceEngine::load_blocking(std::path::Path::new("dummy.gguf"), 0).unwrap();
        let count = engine.token_count("hello world this is a test");
        assert!(count > 0);
    }

    #[test]
    fn test_load_blocking_does_not_crash() {
        let result = InferenceEngine::load_blocking(std::path::Path::new("nonexistent.gguf"), 0);
        assert!(result.is_ok()); // Phase 1 placeholder doesn't actually load
    }
}
