use anyhow::Result;
use async_trait::async_trait;
use openloom_models::ContentPart;
use openloom_models::GpuInfo;
use openloom_models::Message;
use openloom_models::ModelBackend;
use openloom_models::ToolCall;
use openloom_models::ToolChoice;
use openloom_models::ToolDefinition;
use reqwest::Client as HttpClient;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Structured messages array (system/user/assistant/tool).
    pub messages: Vec<Message>,
    /// Tool definitions sent to the API.
    pub tools: Vec<ToolDefinition>,
    /// Tool choice mode.
    pub tool_choice: Option<ToolChoice>,

    // Legacy: flat prompt string (kept for backward compat).
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub stop: Vec<String>,
    pub stream: bool,
    pub thinking_budget: Option<usize>,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            tools: Vec::new(),
            tool_choice: None,
            prompt: String::new(),
            max_tokens: 4096,
            temperature: 0.7,
            top_p: 1.0,
            stop: Vec::new(),
            stream: false,
            thinking_budget: None,
        }
    }
}

impl CompletionRequest {
    /// Get the effective messages array: if messages is non-empty use it,
    /// otherwise convert the legacy flat prompt into a single user message.
    pub fn effective_messages(&self) -> Vec<Message> {
        if !self.messages.is_empty() {
            self.messages.clone()
        } else if !self.prompt.is_empty() {
            vec![Message::user(&self.prompt)]
        } else {
            vec![]
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,
    pub latency_ms: u64,
}

#[derive(Debug)]
pub struct InferenceEngine {
    _model_path: std::path::PathBuf,
    _n_gpu_layers: usize,
}

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

    /// Create a no-op engine when no GGUF model is available.
    pub fn dummy() -> Self {
        Self { _model_path: std::path::PathBuf::new(), _n_gpu_layers: 0 }
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
        tool_calls: Vec::new(),
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
        let messages = self.lower_messages(&req.effective_messages());
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": messages,
        });
        if !req.tools.is_empty() {
            let anthropic_tools: Vec<serde_json::Value> = req
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema,
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(anthropic_tools);
        }
        if let Some(ref tc) = req.tool_choice {
            match tc {
                ToolChoice::Auto => {
                    body["tool_choice"] = serde_json::json!({"type": "auto"});
                }
                ToolChoice::None => {
                    body["tools"] = serde_json::json!([]);
                }
                ToolChoice::Required => {
                    body["tool_choice"] = serde_json::json!({"type": "any"});
                }
            }
        }
        if let Some(budget) = req.thinking_budget {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget,
            });
        }

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

        let (text, tool_calls) = self.parse_anthropic_content(&json);
        let prompt_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize;
        let cached_tokens = json["usage"]["cache_read_input_tokens"]
            .as_u64()
            .unwrap_or(0) as usize;

        Ok(CompletionResponse {
            text,
            tool_calls,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            latency_ms: 0,
        })
    }

    /// Convert canonical Messages to Anthropic wire format.
    fn lower_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .map(|msg| {
                let role = msg.role.as_str();
                let content: Vec<serde_json::Value> = msg
                    .content
                    .iter()
                    .map(|part| match part {
                        ContentPart::Text { text } => serde_json::json!({
                            "type": "text", "text": text
                        }),
                        ContentPart::ToolCall {
                            id,
                            name,
                            arguments,
                        } => serde_json::json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": arguments,
                        }),
                        ContentPart::ToolResult {
                            tool_call_id,
                            name: _,
                            result,
                        } => serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": result,
                        }),
                    })
                    .collect();
                serde_json::json!({ "role": role, "content": content })
            })
            .collect()
    }

    /// Parse Anthropic response content blocks into text + tool_calls.
    fn parse_anthropic_content(&self, json: &serde_json::Value) -> (String, Vec<ToolCall>) {
        let content = json["content"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[]);

        let texts: Vec<String> = content
            .iter()
            .filter_map(|block| {
                if block["type"].as_str() == Some("text") {
                    block["text"].as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        let text = texts.join("\n");

        let tool_calls: Vec<ToolCall> = content
            .iter()
            .filter(|block| matches!(block["type"].as_str(), Some("tool_use")))
            .filter_map(|block| {
                Some(ToolCall {
                    id: block["id"].as_str()?.to_string(),
                    name: block["name"].as_str()?.to_string(),
                    arguments: block["input"].clone(),
                })
            })
            .collect();

        (text, tool_calls)
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
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": [{"role": "user", "content": req.prompt}],
            "stream": true,
        });
        if let Some(budget) = req.thinking_budget {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget,
            });
        }

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
        let messages = self.lower_messages(&req.effective_messages());
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": messages,
        });
        if !req.tools.is_empty() {
            let openai_tools: Vec<serde_json::Value> = req
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema,
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(openai_tools);
        }
        if let Some(ref tc) = req.tool_choice {
            match tc {
                ToolChoice::Auto => {
                    body["tool_choice"] = serde_json::json!("auto");
                }
                ToolChoice::None => {
                    body["tool_choice"] = serde_json::json!("none");
                }
                ToolChoice::Required => {
                    body["tool_choice"] = serde_json::json!("required");
                }
            }
        }
        if req.temperature > 0.0 {
            body["temperature"] = serde_json::json!(req.temperature);
        }

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

        let choice = &json["choices"][0]["message"];
        let text = choice["content"].as_str().unwrap_or("").to_string();
        let tool_calls: Vec<ToolCall> = choice["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        Some(ToolCall {
                            id: tc["id"].as_str()?.to_string(),
                            name: tc["function"]["name"].as_str()?.to_string(),
                            arguments: serde_json::from_str(
                                tc["function"]["arguments"].as_str().unwrap_or("{}"),
                            )
                            .unwrap_or(serde_json::Value::Object(Default::default())),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let prompt_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize;
        let cached_tokens = json["usage"]["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0) as usize;

        Ok(CompletionResponse {
            text,
            tool_calls,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            latency_ms: 0,
        })
    }

    /// Convert canonical Messages to OpenAI Chat Completions wire format.
    fn lower_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
        let result: Vec<serde_json::Value> = messages
            .iter()
            .map(|msg| {
                let role = msg.role.as_str();
                let mut obj = serde_json::json!({ "role": role });

                if role == "assistant" {
                    // DeepSeek requires reasoning_content on every assistant message (even empty)
                    obj["reasoning_content"] = serde_json::json!("");
                }

                if role == "tool" {
                    if let Some(ContentPart::ToolResult {
                        tool_call_id,
                        name: _,
                        result,
                    }) = msg.content.first()
                    {
                        obj["tool_call_id"] = serde_json::json!(tool_call_id);
                        obj["content"] = serde_json::json!(result);
                    }
                    return obj;
                }

                let texts: Vec<&str> = msg
                    .content
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();

                let tool_calls: Vec<serde_json::Value> = msg
                    .content
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::ToolCall {
                            id,
                            name,
                            arguments,
                        } => Some(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(arguments).unwrap_or_default(),
                            }
                        })),
                        _ => None,
                    })
                    .collect();

                if texts.is_empty() && tool_calls.is_empty() {
                    obj["content"] = serde_json::json!("");
                } else if !texts.is_empty() {
                    obj["content"] = serde_json::json!(texts.join("\n"));
                    if !tool_calls.is_empty() {
                        obj["tool_calls"] = serde_json::json!(tool_calls);
                    }
                } else {
                    obj["content"] = serde_json::Value::Null;
                    obj["tool_calls"] = serde_json::json!(tool_calls);
                }

                obj
            })
            .collect();
        result
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
        let messages = self.lower_messages(&req.effective_messages());
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if req.temperature > 0.0 {
            body["temperature"] = serde_json::json!(req.temperature);
        }
        if !req.tools.is_empty() {
            let openai_tools: Vec<serde_json::Value> = req
                .tools
                .iter()
                .map(|t| serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                }))
                .collect();
            body["tools"] = serde_json::json!(openai_tools);
            body["tool_choice"] = serde_json::json!("auto");
        }

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
                            let delta = &val["choices"][0]["delta"];

                            // ─── Reasoning/thinking content (DeepSeek-R1, o1/o3) ───
                            // Sent with special prefix so the forwarder can convert to
                            // ReasoningSummaryTextDelta without the content leaking as text.
                            if let Some(reasoning) = delta["reasoning_content"].as_str() {
                                if !reasoning.is_empty() {
                                    let marker = format!("\x02REASONING\x02{}", reasoning);
                                    if tx.send(marker).await.is_err() {
                                        return Ok(());
                                    }
                                }
                            }

                            // ─── Normal text token ───
                            if let Some(text) = delta["content"].as_str()
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
    }
}

/// Call LM Studio's load model API to ensure a model is loaded before inference.
pub async fn ensure_lm_studio_model(
    base_url: &str,
    model: &str,
    context_size: usize,
) -> Result<()> {
    let load_url = format!("{}/api/v1/models/load", base_url.trim_end_matches("/v1"));
    let client = HttpClient::new();
    let body = serde_json::json!({
        "model": model,
        "context_length": context_size,
    });
    let resp = client.post(&load_url).json(&body).send().await?;
    if resp.status().is_success() {
        tracing::info!(%model, "LM Studio model loaded successfully");
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        // Don't fail — model might already be loaded
        tracing::debug!(%model, %status, %text, "LM Studio load model response (non-fatal)");
    }
    Ok(())
}

#[cfg(test)]
mod cloud_tests {
    use super::*;

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
        assert_eq!(req.max_tokens, 4096);
        assert!((req.temperature - 0.7).abs() < 0.01);
        assert!(!req.stream);
        assert!(req.thinking_budget.is_none());
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
