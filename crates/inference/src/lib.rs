use anyhow::Result;
use async_trait::async_trait;
use openloom_models::GpuInfo;
use openloom_models::ModelBackend;
use reqwest::Client as HttpClient;
use std::path::Path;

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
            max_tokens: 4096,
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
    pub cached_tokens: usize,
    pub latency_ms: u64,
}

#[cfg(feature = "llama")]
struct LlamaRuntime {
    model: llama_cpp_2::model::LlamaModel,
    ctx: std::cell::RefCell<llama_cpp_2::context::LlamaContext<'static>>,
    sampler: std::cell::RefCell<llama_cpp_2::sampling::LlamaSampler>,
}

// SAFETY: llama.cpp types are internally thread-safe. LlamaRuntime is the
// exclusive owner and inference is serialized via RefCell borrow checking.
#[cfg(feature = "llama")]
unsafe impl Send for LlamaRuntime {}
#[cfg(feature = "llama")]
unsafe impl Sync for LlamaRuntime {}

#[cfg(feature = "llama")]
pub struct InferenceEngine {
    runtime: Option<std::sync::Arc<LlamaRuntime>>,
}

#[cfg(feature = "llama")]
impl std::fmt::Debug for InferenceEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InferenceEngine")
            .field("runtime", &self.runtime.is_some())
            .finish()
    }
}
#[cfg(not(feature = "llama"))]
#[derive(Debug)]
pub struct InferenceEngine {
    _model_path: std::path::PathBuf,
    _n_gpu_layers: usize,
}

// === llama feature implementation ===

#[cfg(feature = "llama")]
fn generate(
    model: &llama_cpp_2::model::LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    sampler: &mut llama_cpp_2::sampling::LlamaSampler,
    prompt: &str,
    max_tokens: usize,
) -> String {
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::AddBos;
    use llama_cpp_2::token::LlamaToken;

    let tokens = match model.str_to_token(prompt, AddBos::Always) {
        Ok(t) => t,
        Err(_) => return String::new(),
    };

    // Clear KV cache from previous conversation turns so positions start fresh
    ctx.clear_kv_cache();

    let mut batch = LlamaBatch::new(tokens.len() + max_tokens, 1);
    for (i, &token) in tokens.iter().enumerate() {
        let _ = batch.add(token, i as i32, &[0], i == tokens.len() - 1);
    }

    let mut result = String::with_capacity(max_tokens * 4);
    let eos = model.token_eos();
    let mut sample_idx = (tokens.len() - 1) as i32;
    for (n_generated, _) in (0_i32..).zip(0..max_tokens) {
        if ctx.decode(&mut batch).is_err() {
            break;
        }
        batch = LlamaBatch::new(1, 1);

        let token = sampler.sample(ctx, sample_idx);
        sample_idx = 0; // subsequent batches have 1 token
        if token == eos || token == LlamaToken(0) {
            break;
        }

        if let Ok(bytes) = model.token_to_piece_bytes(token, 32, false, None)
            && let Ok(piece) = String::from_utf8(bytes)
        {
            result.push_str(&piece);
        }
        let _ = batch.add(token, tokens.len() as i32 + n_generated, &[0], true);
    }

    result
}

#[cfg(feature = "llama")]
impl InferenceEngine {
    pub async fn load(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        Self::load_blocking(model_path, n_gpu_layers)
    }

    pub fn load_blocking(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        use llama_cpp_2::context::params::LlamaContextParams;
        use llama_cpp_2::llama_backend::LlamaBackend;
        use llama_cpp_2::model::params::LlamaModelParams;
        use llama_cpp_2::sampling::LlamaSampler;
        use std::cell::RefCell;
        use std::num::NonZeroU32;

        if !model_path.exists() {
            tracing::warn!(path = %model_path.display(), "model file not found, inference unavailable");
            return Ok(Self { runtime: None });
        }

        let backend =
            LlamaBackend::init().map_err(|e| anyhow::anyhow!("llama backend init: {}", e))?;

        let model = llama_cpp_2::model::LlamaModel::load_from_file(
            &backend,
            model_path,
            &LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers as u32),
        )
        .map_err(|e| anyhow::anyhow!("model load: {}", e))?;

        let ctx = model
            .new_context(
                &backend,
                LlamaContextParams::default().with_n_ctx(NonZeroU32::new(4096)),
            )
            .map_err(|e| anyhow::anyhow!("context create: {}", e))?;

        let sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.7),
            LlamaSampler::top_p(0.95, 1),
            LlamaSampler::greedy(),
        ]);

        // Coerce to 'static: safe because both model and context live together in Rc
        let ctx: llama_cpp_2::context::LlamaContext<'static> = unsafe { std::mem::transmute(ctx) };

        let runtime = std::sync::Arc::new(LlamaRuntime {
            model,
            ctx: RefCell::new(ctx),
            sampler: RefCell::new(sampler),
        });

        tracing::info!(path = %model_path.display(), n_gpu_layers, "llama model loaded");
        Ok(Self {
            runtime: Some(runtime),
        })
    }

    pub async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let Some(runtime) = &self.runtime else {
            return Ok(stub_complete(&req));
        };
        let runtime = runtime.clone();
        tokio::task::spawn_blocking(move || {
            use llama_cpp_2::model::AddBos;
            let prompt_tokens = runtime
                .model
                .str_to_token(&req.prompt, AddBos::Always)
                .map(|t| t.len())
                .unwrap_or(req.prompt.chars().count() / 4);
            let text = generate(
                &runtime.model,
                &mut runtime.ctx.borrow_mut(),
                &mut runtime.sampler.borrow_mut(),
                &req.prompt,
                req.max_tokens,
            );
            let completion_tokens = text.chars().count() / 4;
            Ok(CompletionResponse {
                text,
                prompt_tokens,
                completion_tokens,
                cached_tokens: 0,
                latency_ms: 0,
            })
        })
        .await
        .map_err(|e| anyhow::anyhow!("join error: {}", e))?
    }

    pub async fn complete_stream(
        &self,
        req: CompletionRequest,
        token_tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        let Some(runtime) = &self.runtime else {
            let resp = self.complete(req).await?;
            let _ = token_tx.send(resp.text).await;
            return Ok(());
        };
        let runtime = runtime.clone();
        let text = tokio::task::spawn_blocking(move || {
            generate(
                &runtime.model,
                &mut runtime.ctx.borrow_mut(),
                &mut runtime.sampler.borrow_mut(),
                &req.prompt,
                req.max_tokens,
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("join error: {}", e))?;
        let _ = token_tx.send(text).await;
        Ok(())
    }

    pub fn token_count(&self, text: &str) -> usize {
        let Some(runtime) = &self.runtime else {
            return text.chars().count() / 4;
        };
        use llama_cpp_2::model::AddBos;
        runtime
            .model
            .str_to_token(text, AddBos::Never)
            .map(|t| t.len())
            .unwrap_or(text.chars().count() / 4)
    }
}

// === stub (non-llama) implementation ===

#[cfg(not(feature = "llama"))]
impl InferenceEngine {
    pub async fn load(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        tracing::info!(path = %model_path.display(), n_gpu_layers, "loading model");
        Ok(Self {
            _model_path: model_path.to_path_buf(),
            _n_gpu_layers: if n_gpu_layers > 0 { n_gpu_layers } else { 0 },
        })
    }

    pub fn load_blocking(model_path: &Path, n_gpu_layers: usize) -> Result<Self> {
        tracing::info!(path = %model_path.display(), n_gpu_layers, "loading model (sync)");
        Ok(Self {
            _model_path: model_path.to_path_buf(),
            _n_gpu_layers: n_gpu_layers,
        })
    }

    pub async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        Ok(stub_complete(&req))
    }

    pub async fn complete_stream(
        &self,
        req: CompletionRequest,
        token_tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        let resp = self.complete(req).await?;
        let _ = token_tx.send(resp.text).await;
        Ok(())
    }

    pub fn token_count(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
}

// === shared helpers and functions ===

fn stub_complete(req: &CompletionRequest) -> CompletionResponse {
    let prompt_chars = req.prompt.chars().count();
    let response = "[openLoom] No inference backend available.\n\nConfigure a model in config.toml:\n  - backend = \"LmStudio\" (http://localhost:1234)\n  - backend = \"Ollama\" (http://localhost:11434)\n  - backend = \"Anthropic\" / \"OpenAI\" / \"DeepSeek\" (cloud API)\n\nRun `openloom doctor` for setup help.".to_string();
    let response_tokens = response.chars().count() / 4;
    CompletionResponse {
        text: response,
        prompt_tokens: prompt_chars / 4,
        completion_tokens: response_tokens,
        cached_tokens: 0,
        latency_ms: 0,
    }
}

impl InferenceEngine {
    pub fn detect_gpu() -> GpuInfo {
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
    base_url: String,
    http: HttpClient,
}

impl AnthropicClient {
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
            .post(format!("{}/v1/messages", self.base_url))
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

        let body_text = resp.text().await?;
        let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            let preview = &body_text[..body_text.len().min(500)];
            anyhow::anyhow!("Anthropic response parse error: {}, body: {}", e, preview)
        })?;
        let text = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let prompt_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize;
        let cached_tokens = json["usage"]["cache_read_input_tokens"]
            .as_u64()
            .unwrap_or(0) as usize;

        Ok(CompletionResponse {
            text,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
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
            .post(format!("{}/v1/messages", self.base_url))
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
        let mut prompt_tokens: u64 = 0;
        let mut completion_tokens: u64 = 0;
        let mut cached_tokens: u64 = 0;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buffer.find("\n\n") {
                let frame = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                for line in frame.lines() {
                    if let Some(data) = line.strip_prefix("data: ")
                        && let Ok(val) = serde_json::from_str::<serde_json::Value>(data)
                    {
                        // Stream text tokens
                        if let Some(text) = val["delta"]["text"].as_str()
                            && tx.send(text.to_string()).await.is_err()
                        {
                            return Ok(());
                        }
                        // message_start event has input token usage
                        if let Some(usage) = val.get("message").and_then(|m| m.get("usage")) {
                            prompt_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
                            cached_tokens = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
                        }
                        // message_delta event has output token usage
                        if let Some(usage) = val.get("usage") {
                            completion_tokens =
                                usage["output_tokens"].as_u64().unwrap_or(completion_tokens);
                        }
                    }
                }
            }
        }

        // Emit usage signal
        if prompt_tokens > 0 || completion_tokens > 0 {
            let usage_msg = format!(
                "\x00USAGE:{}:{}:{}",
                prompt_tokens, completion_tokens, cached_tokens
            );
            let _ = tx.send(usage_msg).await;
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

        let body_text = resp.text().await?;
        let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            let preview = &body_text[..body_text.len().min(500)];
            anyhow::anyhow!("API response parse error: {}, body: {}", e, preview)
        })?;
        let text = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let prompt_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize;
        let cached_tokens = json["usage"]["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0) as usize;

        Ok(CompletionResponse {
            text,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
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
            "stream_options": {"include_usage": true},
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
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                            // Stream text tokens
                            if let Some(text) = val["choices"][0]["delta"]["content"].as_str()
                                && tx.send(text.to_string()).await.is_err()
                            {
                                return Ok(());
                            }
                            // Parse usage from final chunk (stream_options: include_usage)
                            if let Some(usage) = val.get("usage")
                                && usage.is_object()
                            {
                                let prompt_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0);
                                let completion_tokens =
                                    usage["completion_tokens"].as_u64().unwrap_or(0);
                                let cached_tokens = usage["prompt_tokens_details"]["cached_tokens"]
                                    .as_u64()
                                    .unwrap_or(0);
                                let usage_msg = format!(
                                    "\x00USAGE:{}:{}:{}",
                                    prompt_tokens, completion_tokens, cached_tokens
                                );
                                let _ = tx.send(usage_msg).await;
                            }
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
    // API key: try env var, fall back to empty string (LM Studio/Ollama don't need one)
    let api_key = config
        .api_key_env
        .as_deref()
        .and_then(|env_name| {
            if env_name.is_empty() {
                None
            } else {
                std::env::var(env_name).ok()
            }
        })
        .unwrap_or_default();

    // For cloud backends that require a key, error if empty
    if api_key.is_empty()
        && matches!(
            config.backend,
            ModelBackend::Anthropic | ModelBackend::OpenAI | ModelBackend::DeepSeek
        )
    {
        anyhow::bail!(
            "API key not set for {} (env: {:?})",
            config.backend.name(),
            config.api_key_env
        );
    }

    // Strip [1m] etc. suffix — it's a client-side context-size hint, not an API model name
    let model = config
        .model
        .clone()
        .unwrap_or_default()
        .split('[')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if model.is_empty() {
        anyhow::bail!("model name not configured");
    }
    let base_url = match config.base_url.clone() {
        Some(url) => url.trim().trim_end_matches('/').to_string(),
        None => match config.backend {
            ModelBackend::Anthropic => "https://api.anthropic.com".into(),
            ModelBackend::DeepSeek => "https://api.deepseek.com".into(),
            ModelBackend::LmStudio => "http://localhost:1234/v1".into(),
            ModelBackend::Ollama => "http://localhost:11434/v1".into(),
            _ => "https://api.openai.com".into(),
        },
    };
    match config.backend {
        ModelBackend::Anthropic => Ok(Box::new(AnthropicClient::new(api_key, model, base_url))),
        ModelBackend::OpenAI => Ok(Box::new(OpenAIClient::new(api_key, model, base_url))),
        ModelBackend::DeepSeek => Ok(Box::new(OpenAIClient::new(api_key, model, base_url))),
        ModelBackend::LmStudio => Ok(Box::new(OpenAIClient::new(api_key, model, base_url))),
        ModelBackend::Ollama => Ok(Box::new(OpenAIClient::new(api_key, model, base_url))),
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
        let client: Box<dyn CloudClient> = Box::new(AnthropicClient::new(
            "key".into(),
            "claude".into(),
            "https://api.anthropic.com".into(),
        ));
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
        assert!(result.is_ok());
    }
}
