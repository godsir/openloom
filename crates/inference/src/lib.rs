use anyhow::Result;
use async_trait::async_trait;
use openloom_models::GpuInfo;
use openloom_models::ModelBackend;
use reqwest::Client as HttpClient;
use std::path::Path;

#[cfg(feature = "llama")]
use std::sync::mpsc;
#[cfg(feature = "llama")]
use tokio::sync::oneshot;

#[cfg(feature = "llama")]
enum EngineCommand {
    Complete {
        prompt: String,
        max_tokens: usize,
        temperature: f32,
        reply: oneshot::Sender<CompletionResponse>,
    },
    CompleteStream {
        prompt: String,
        max_tokens: usize,
        token_tx: mpsc::Sender<String>,
    },
    TokenCount {
        text: String,
        reply: oneshot::Sender<usize>,
    },
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub stop: Vec<String>,
    pub stream: bool,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 1.0,
            stop: Vec::new(),
            stream: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub latency_ms: u64,
}

#[cfg(feature = "llama")]
#[derive(Debug)]
pub struct InferenceEngine {
    sender: Option<mpsc::Sender<EngineCommand>>,
}

#[cfg(not(feature = "llama"))]
#[derive(Debug)]
pub struct InferenceEngine {
    _model_path: std::path::PathBuf,
    _n_gpu_layers: usize,
}

// === llama feature implementation ===

#[cfg(feature = "llama")]
impl InferenceEngine {
    /// Load GGUF model asynchronously (delegates to load_blocking).
    pub async fn load(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        Self::load_blocking(model_path, n_gpu_layers)
    }

    /// Synchronous load: initializes llama backend, loads model, spawns worker thread.
    pub fn load_blocking(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        use llama_cpp_2::*;
        use std::num::NonZeroU32;

        if !model_path.exists() {
            tracing::warn!(path = %model_path.display(), "model file not found, inference unavailable");
            return Ok(Self { sender: None });
        }

        let backend = LlamaBackend::init()
            .map_err(|e| anyhow::anyhow!("llama backend init: {}", e))?;

        let model = LlamaModel::load_from_file(
            &backend,
            model_path,
            &LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers as u32),
        )
        .map_err(|e| anyhow::anyhow!("model load: {}", e))?;

        let ctx = model
            .new_context(
                &backend,
                LlamaContextParams::default().with_n_ctx(NonZeroU32::new(4096).unwrap()),
            )
            .map_err(|e| anyhow::anyhow!("context create: {}", e))?;

        let (tx, rx) = mpsc::channel::<EngineCommand>();

        std::thread::Builder::new()
            .name("llama-worker".into())
            .spawn(move || {
                let sampler = LlamaSampler::chain_simple([
                    LlamaSampler::temp(0.7),
                    LlamaSampler::top_p(0.95),
                    LlamaSampler::greedy(),
                ]);
                for cmd in rx {
                    match cmd {
                        EngineCommand::Complete {
                            prompt,
                            max_tokens,
                            temperature: _,
                            reply,
                        } => {
                            let prompt_tokens = model
                                .str_to_token(&prompt, AddBos::Always)
                                .map(|t| t.len())
                                .unwrap_or(prompt.chars().count() / 4);
                            let result =
                                model.create_completion(&prompt, max_tokens as u32);
                            let text = result.unwrap_or_default();
                            let completion_tokens = text.chars().count() / 4;
                            let _ = reply.send(CompletionResponse {
                                text,
                                prompt_tokens,
                                completion_tokens,
                                latency_ms: 0,
                            });
                        }
                        EngineCommand::CompleteStream {
                            prompt,
                            max_tokens,
                            token_tx,
                        } => {
                            let result =
                                model.create_completion(&prompt, max_tokens as u32);
                            let _ = token_tx.send(result.unwrap_or_default());
                        }
                        EngineCommand::TokenCount { text, reply } => {
                            let count = model
                                .str_to_token(&text, AddBos::Never)
                                .map(|t| t.len())
                                .unwrap_or(text.chars().count() / 4);
                            let _ = reply.send(count);
                        }
                    }
                }
            })?;

        tracing::info!(path = %model_path.display(), n_gpu_layers, "llama model loaded");
        Ok(Self {
            sender: Some(tx),
        })
    }

    /// Complete text completion via worker thread. Falls back to stub if no model is loaded.
    pub async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let Some(sender) = &self.sender else {
            return Ok(stub_complete(&req));
        };

        let (reply_tx, reply_rx) = oneshot::channel();
        sender
            .send(EngineCommand::Complete {
                prompt: req.prompt,
                max_tokens: req.max_tokens,
                temperature: req.temperature,
                reply: reply_tx,
            })
            .map_err(|e| anyhow::anyhow!("engine channel closed: {}", e))?;

        reply_rx
            .await
            .map_err(|e| anyhow::anyhow!("worker thread dropped: {}", e))
    }

    /// Streaming completion via worker thread. Falls back to whole response if no model loaded.
    pub async fn complete_stream(
        &self,
        req: CompletionRequest,
        token_tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        let Some(sender) = &self.sender else {
            let resp = self.complete(req).await?;
            let _ = token_tx.send(resp.text).await;
            return Ok(());
        };

        let (std_tx, std_rx) = mpsc::channel();
        sender
            .send(EngineCommand::CompleteStream {
                prompt: req.prompt,
                max_tokens: req.max_tokens,
                token_tx: std_tx,
            })
            .map_err(|e| anyhow::anyhow!("engine channel closed: {}", e))?;

        let text = tokio::task::spawn_blocking(move || {
            std_rx
                .recv()
                .map_err(|e| anyhow::anyhow!("worker recv error: {}", e))
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

        let _ = token_tx.send(text).await;
        Ok(())
    }

    /// Count tokens using the loaded model's tokenizer, or fallback estimation.
    pub fn token_count(&self, text: &str) -> usize {
        let Some(sender) = &self.sender else {
            return text.chars().count() / 4;
        };

        let (reply_tx, reply_rx) = oneshot::channel();
        if sender
            .send(EngineCommand::TokenCount {
                text: text.to_string(),
                reply: reply_tx,
            })
            .is_err()
        {
            return text.chars().count() / 4;
        }

        reply_rx.blocking_recv().unwrap_or(text.chars().count() / 4)
    }
}

// === stub (non-llama) implementation ===

#[cfg(not(feature = "llama"))]
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
        Ok(stub_complete(&req))
    }

    /// Streaming completion: calls complete() and sends the result text.
    pub async fn complete_stream(
        &self,
        req: CompletionRequest,
        token_tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        let resp = self.complete(req).await?;
        let _ = token_tx.send(resp.text).await;
        Ok(())
    }

    /// Count tokens in text using a simple estimation (~4 chars per token).
    pub fn token_count(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
}

// === shared helpers and functions ===

/// Stub response used when no model is available (both cfg variants).
fn stub_complete(req: &CompletionRequest) -> CompletionResponse {
    let prompt_chars = req.prompt.chars().count();
    let preview: String = req.prompt.chars().take(100).collect();
    let response = format!(
        "[openLoom] Local model (Qwen3-1.7B) is not yet loaded. Install the GGUF model file to enable inference.\n\nYour message ({} chars): {}...",
        prompt_chars, preview
    );
    let response_tokens = response.chars().count() / 4;
    CompletionResponse {
        text: response,
        prompt_tokens: prompt_chars / 4,
        completion_tokens: response_tokens,
        latency_ms: 0,
    }
}

impl InferenceEngine {
    /// Detect GPU info (vendor, VRAM, support status).
    /// Uses nvidia-smi on Windows/Linux. Returns "none" fallback if no GPU found.
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
                let vendor = parts
                    .first()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
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
}

// === Cloud Client ===

#[async_trait]
pub trait CloudClient: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> anyhow::Result<CompletionResponse>;
    async fn complete_stream(
        &self,
        req: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()>;
    fn provider(&self) -> ModelBackend;
    fn model_name(&self) -> &str;
}

pub struct AnthropicClient {
    api_key: String,
    model: String,
    http: HttpClient,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            http: HttpClient::new(),
        }
    }

    async fn complete_with_retry(
        &self,
        req: &CompletionRequest,
        retries: usize,
    ) -> anyhow::Result<CompletionResponse> {
        let mut last_err = None;
        for attempt in 0..=retries {
            if attempt > 0 {
                let delay = 2u64.pow(attempt as u32) * 500;
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            match self.try_complete(req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "Anthropic API call failed");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap())
    }

    async fn try_complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": [{"role": "user", "content": &req.prompt}],
        });

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let prompt_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize;

        Ok(CompletionResponse {
            text,
            prompt_tokens,
            completion_tokens,
            latency_ms: 0,
        })
    }
}

#[async_trait]
impl CloudClient for AnthropicClient {
    async fn complete(&self, req: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        self.complete_with_retry(&req, 3).await
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": [{"role": "user", "content": req.prompt}],
            "stream": true,
        });

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {}: {}", status, text);
        }

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buffer.find("\n\n") {
                let frame = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                for line in frame.lines() {
                    if let Some(data) = line.strip_prefix("data: ")
                        && let Ok(val) = serde_json::from_str::<serde_json::Value>(data)
                        && let Some(text) = val["delta"]["text"].as_str()
                        && tx.send(text.to_string()).await.is_err()
                    {
                        return Ok(()); // client disconnected
                    }
                }
            }
        }
        Ok(())
    }

    fn provider(&self) -> ModelBackend {
        ModelBackend::Anthropic
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

pub struct OpenAIClient {
    api_key: String,
    model: String,
    base_url: String,
    http: HttpClient,
}

impl OpenAIClient {
    pub fn new(api_key: String, model: String, base_url: String) -> Self {
        Self {
            api_key,
            model,
            base_url,
            http: HttpClient::new(),
        }
    }

    async fn complete_with_retry(
        &self,
        req: &CompletionRequest,
        retries: usize,
    ) -> anyhow::Result<CompletionResponse> {
        let mut last_err = None;
        for attempt in 0..=retries {
            if attempt > 0 {
                let delay = 2u64.pow(attempt as u32) * 500;
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            match self.try_complete(req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "API call failed");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap())
    }

    async fn try_complete(&self, req: &CompletionRequest) -> anyhow::Result<CompletionResponse> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": [{"role": "user", "content": &req.prompt}],
        });

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let text = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let prompt_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize;

        Ok(CompletionResponse {
            text,
            prompt_tokens,
            completion_tokens,
            latency_ms: 0,
        })
    }
}

#[async_trait]
impl CloudClient for OpenAIClient {
    async fn complete(&self, req: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        self.complete_with_retry(&req, 3).await
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": [{"role": "user", "content": req.prompt}],
            "stream": true,
        });

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buffer.find("\n\n") {
                let frame = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                for line in frame.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            return Ok(());
                        }
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data)
                            && let Some(text) = val["choices"][0]["delta"]["content"].as_str()
                            && tx.send(text.to_string()).await.is_err()
                        {
                            return Ok(()); // client disconnected
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn provider(&self) -> ModelBackend {
        ModelBackend::OpenAI
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

pub fn create_cloud_client(
    config: &openloom_models::ModelConfig,
) -> anyhow::Result<Box<dyn CloudClient>> {
    let api_key = std::env::var(config.api_key_env.as_deref().unwrap_or(""))
        .map_err(|_| anyhow::anyhow!("API key env var not set"))?;
    let model = config.model.clone().unwrap_or_default();
    if model.is_empty() {
        anyhow::bail!("model name not configured");
    }
    match config.backend {
        ModelBackend::Anthropic => Ok(Box::new(AnthropicClient::new(api_key, model))),
        ModelBackend::OpenAI => Ok(Box::new(OpenAIClient::new(
            api_key,
            model,
            "https://api.openai.com/v1".into(),
        ))),
        ModelBackend::DeepSeek => Ok(Box::new(OpenAIClient::new(
            api_key,
            model,
            "https://api.deepseek.com/v1".into(),
        ))),
        ModelBackend::LlamaCpp => anyhow::bail!("LlamaCpp is not a cloud backend"),
    }
}

#[cfg(test)]
mod cloud_tests {
    use super::*;

    #[test]
    fn test_create_cloud_client_llama_errors() {
        let config = openloom_models::ModelConfig {
            backend: ModelBackend::LlamaCpp,
            ..Default::default()
        };
        assert!(create_cloud_client(&config).is_err());
    }

    #[test]
    fn test_create_cloud_client_missing_api_key() {
        let config = openloom_models::ModelConfig {
            backend: ModelBackend::Anthropic,
            model: Some("claude-sonnet-4-6".into()),
            api_key_env: Some("NONEXISTENT_ENV_VAR_12345".into()),
            ..Default::default()
        };
        assert!(create_cloud_client(&config).is_err());
    }

    #[test]
    fn test_cloud_client_trait_object() {
        let client: Box<dyn CloudClient> =
            Box::new(AnthropicClient::new("key".into(), "claude".into()));
        assert_eq!(client.provider(), ModelBackend::Anthropic);
        assert_eq!(client.model_name(), "claude");
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
