//! InferenceEngine — local inference via OpenAI-compatible HTTP API (LM Studio / Ollama)
//! and CloudClient trait for provider dispatch.

use crate::cache::PrefixCache;
use anyhow::Result;
use async_trait::async_trait;
use loom_types::{
    CompletionRequest, CompletionResponse, ContentPart, GpuInfo, Message, ModelBackend,
    StreamDelta, ToolCall,
};
use reqwest::Client as HttpClient;
use tokio::sync::mpsc;

/// Local inference engine backed by an OpenAI-compatible HTTP endpoint.
///
/// Connects to LM Studio (default `http://localhost:1234/v1`) or Ollama
/// (`http://localhost:11434/v1`) and implements the `CloudClient` trait
/// so it slots directly into the agent loop.
pub struct InferenceEngine {
    base_url: String,
    model: String,
    http: HttpClient,
    pub prefix_cache: PrefixCache,
}

impl std::fmt::Debug for InferenceEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InferenceEngine")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl InferenceEngine {
    /// Connect to a local endpoint and trigger model loading if supported.
    pub async fn connect(base_url: &str, model: &str, _context_size: usize) -> Result<Self> {
        let base = base_url.trim_end_matches('/').to_string();
        let http = HttpClient::new();

        // Check if the model is already loaded before triggering a redundant load
        let already_loaded = match http
            .get(format!("{}/models", base))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(v) => v
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|a| a.iter().any(|m| m.get("id").and_then(|id| id.as_str()) == Some(model)))
                    .unwrap_or(false),
                Err(_) => false,
            },
            Err(_) => false,
        };

        if already_loaded {
            tracing::info!(%model, "model already loaded, skipping load API");
        } else {
            // Trigger model load via LM Studio API (non-fatal if it fails)
            let load_url = base.trim_end_matches("/v1");
            match http
                .post(format!("{}/api/v1/models/load", load_url))
                .json(&serde_json::json!({"model": model}))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!(%model, "model loaded via LM Studio API");
                }
                _ => {
                    tracing::debug!(%model, "model load API skipped (non-LM-Studio or load failed)");
                }
            }
        }

        tracing::info!(%base, %model, "inference engine connected");
        Ok(Self {
            base_url: base,
            model: model.to_string(),
            http,
            prefix_cache: PrefixCache::new(2),
        })
    }

    /// Build an engine pointed at a known endpoint (no load trigger).
    pub fn new(base_url: String, model: String) -> Self {
        let http = HttpClient::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_default();
        Self {
            base_url,
            model,
            http,
            prefix_cache: PrefixCache::new(2),
        }
    }

    /// Dummy engine for tests — will fail at call time.
    pub fn dummy() -> Self {
        Self {
            base_url: "http://localhost:1".into(),
            model: "dummy".into(),
            http: HttpClient::new(),
            prefix_cache: PrefixCache::new(2),
        }
    }

    /// Blocking variant of `connect`.
    pub fn connect_blocking(base_url: &str, model: &str, context_size: usize) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .enable_io()
            .build()?;
        rt.block_on(Self::connect(base_url, model, context_size))
    }

    // ── completion ──────────────────────────────────────────────

    /// Send a completion request to the local endpoint.
    pub async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let messages = lower_messages(&req.effective_messages());
        let eff = req.effective_messages();
        let (cache_hit, _) = self.prefix_cache.check(&eff);
        if cache_hit {
            tracing::info!("KV cache hit — llama.cpp reuses prefix");
        } else {
            tracing::info!("KV cache miss — cold prefix");
        }

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": messages,
        });
        if !req.tools.is_empty() {
            body["tools"] = build_tools_json(&req.tools);
        }
        if req.temperature > 0.0 {
            body["temperature"] = req.temperature.into();
        }

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Local endpoint error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        let choice = &json["choices"][0]["message"];
        let raw_text = choice["content"].as_str().unwrap_or("").to_string();

        let tool_calls: Vec<ToolCall> = choice["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        let id = tc["id"].as_str().filter(|s| !s.is_empty())?;
                        let name = tc["function"]["name"].as_str().filter(|s| !s.is_empty())?;
                        let raw_args = &tc["function"]["arguments"];
                        tracing::info!(
                            tool_name = %name,
                            args_type = if raw_args.is_string() { "string" } else if raw_args.is_object() { "object" } else { "other" },
                            args_preview = %format!("{:?}", raw_args).chars().take(200).collect::<String>(),
                            "parsing tool call arguments (engine)"
                        );
                        Some(ToolCall {
                            id: id.to_string(),
                            name: name.to_string(),
                            arguments: parse_tool_arguments(raw_args),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            text: if tool_calls.is_empty() {
                raw_text
            } else {
                String::new()
            },
            tool_calls,
            prompt_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize,
            completion_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize,
            cached_tokens: 0,
            latency_ms: 0,
            thinking: choice["reasoning_content"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(String::from),
            images: Vec::new(),
        })
    }

    /// Stream raw tokens via `token_tx`. For structured streaming use `CloudClient::complete_stream_structured`.
    pub async fn complete_stream(
        &self,
        req: CompletionRequest,
        token_tx: mpsc::Sender<String>,
    ) -> Result<()> {
        let messages = lower_messages(&req.effective_messages());
        let eff = req.effective_messages();
        let (cache_hit, _) = self.prefix_cache.check(&eff);
        if cache_hit {
            tracing::info!("KV cache hit (stream)");
        } else {
            tracing::info!("KV cache miss (stream)");
        }
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if !req.tools.is_empty() {
            body["tools"] = build_tools_json(&req.tools);
        }
        if req.temperature > 0.0 {
            body["temperature"] = req.temperature.into();
        }

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Local endpoint error {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.extend_from_slice(&chunk);
            while let Some((pos, skip)) = find_sse_boundary(&buf) {
                let frame_bytes = buf[..pos].to_vec();
                buf.drain(..pos + skip);
                let frame = String::from_utf8_lossy(&frame_bytes);
                for line in frame.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            return Ok(());
                        }
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                            let d = &val["choices"][0]["delta"];
                            if let Some(r) =
                                d["reasoning_content"].as_str().filter(|s| !s.is_empty())
                                && token_tx
                                    .send(format!("\x02REASONING\x02{}", r))
                                    .await
                                    .is_err()
                            {
                                return Ok(());
                            }
                            if let Some(t) = d["content"].as_str()
                                && token_tx.send(t.to_string()).await.is_err()
                            {
                                return Ok(());
                            }
                            if let Some(u) = val.get("usage").filter(|u| u.is_object()) {
                                let _ = token_tx
                                    .send(format!(
                                        "\x00USAGE:{}:{}:{}",
                                        u["prompt_tokens"].as_u64().unwrap_or(0),
                                        u["completion_tokens"].as_u64().unwrap_or(0),
                                        u["prompt_tokens_details"]["cached_tokens"]
                                            .as_u64()
                                            .unwrap_or(0),
                                    ))
                                    .await;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Rough token count (char-based estimate — local models vary).
    pub fn token_count(&self, text: &str) -> usize {
        text.chars().count() / 4
    }

    // ── GPU detection ───────────────────────────────────────────

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

/// Send an unload request to a local inference endpoint (non-fatal).
///
/// Used when switching between LM Studio / Ollama models to avoid
/// piling up multiple models in VRAM.
pub async fn unload_local_model(base_url: &str, model: &str) {
    let base = base_url.trim_end_matches("/v1");
    let url = format!("{}/api/v1/models/unload", base);
    // LM Studio newer versions use "identifier"; older ones used "model".
    // Try with "identifier" first (newer API), silently ignore any failure.
    let body = serde_json::json!({"identifier": model, "model": model});
    match HttpClient::new()
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!(%model, "model unloaded");
        }
        _ => {
            tracing::debug!(%model, "model unload skipped (non-LM-Studio or already unloaded)");
        }
    }
}

/// Find SSE frame boundary: "\n\n" (Linux/macOS) or "\r\n\r\n" (Windows/LM Studio default).
/// Returns (position, skip_bytes).
pub(crate) fn find_sse_boundary(buf: &[u8]) -> Option<(usize, usize)> {
    buf.windows(2)
        .position(|w| w == b"\n\n")
        .map(|pos| (pos, 2))
        .or_else(|| {
            buf.windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|pos| (pos, 4))
        })
}

// ── CloudClient impl ────────────────────────────────────────────────

#[async_trait]
impl CloudClient for InferenceEngine {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.complete(req).await
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
        tx: mpsc::Sender<String>,
    ) -> Result<()> {
        self.complete_stream(req, tx).await
    }

    async fn complete_stream_structured(
        &self,
        req: CompletionRequest,
        tx: mpsc::Sender<StreamDelta>,
    ) -> Result<()> {
        let messages = lower_messages(&req.effective_messages());
        let eff = req.effective_messages();
        let (cache_hit, _) = self.prefix_cache.check(&eff);
        if cache_hit {
            tracing::info!("KV cache hit (structured stream)");
        } else {
            tracing::info!("KV cache miss (structured stream)");
        }

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if !req.tools.is_empty() {
            body["tools"] = build_tools_json(&req.tools);
        }
        if req.temperature > 0.0 {
            body["temperature"] = req.temperature.into();
        }

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Local endpoint error {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.extend_from_slice(&chunk);
            while let Some((pos, skip)) = find_sse_boundary(&buf) {
                let frame_bytes = buf[..pos].to_vec();
                buf.drain(..pos + skip);
                let frame = String::from_utf8_lossy(&frame_bytes);
                for line in frame.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            return Ok(());
                        }
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                            let d = &val["choices"][0]["delta"];
                            if let Some(r) = d["reasoning_content"].as_str()
                                && !r.is_empty()
                                && tx
                                    .send(StreamDelta::Reasoning(r.to_string()))
                                    .await
                                    .is_err()
                            {
                                return Ok(());
                            }
                            if let Some(t) = d["content"].as_str()
                                && !t.is_empty()
                                && tx.send(StreamDelta::Text(t.to_string())).await.is_err()
                            {
                                return Ok(());
                            }
                            if let Some(tcs) = d["tool_calls"].as_array() {
                                for tc in tcs {
                                    let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                                    if let (Some(id), Some(name)) =
                                        (tc["id"].as_str(), tc["function"]["name"].as_str())
                                        && !id.is_empty() && !name.is_empty()
                                        && tx
                                            .send(StreamDelta::ToolCallBegin {
                                                index: idx,
                                                id: id.to_string(),
                                                name: name.to_string(),
                                            })
                                            .await
                                            .is_err()
                                    {
                                        return Ok(());
                                    }
                                    if let Some(args) = tc["function"]["arguments"].as_str()
                                        && !args.is_empty()
                                        && tx
                                            .send(StreamDelta::ToolCallArgsChunk {
                                                index: idx,
                                                chunk: args.to_string(),
                                            })
                                            .await
                                            .is_err()
                                    {
                                        return Ok(());
                                    }
                                }
                            }
                            if let Some(u) = val.get("usage").filter(|u| u.is_object())
                                && tx
                                    .send(StreamDelta::Usage {
                                        prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0),
                                        completion_tokens: u["completion_tokens"]
                                            .as_u64()
                                            .unwrap_or(0),
                                        cache_read_tokens:
                                            u["prompt_tokens_details"]["cached_tokens"]
                                                .as_u64()
                                                .unwrap_or(0),
                                        cache_write_tokens: 0,
                                    })
                                    .await
                                    .is_err()
                            {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn provider(&self) -> ModelBackend {
        ModelBackend::LmStudio
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn prefix_cache_reset(&self) {
        self.prefix_cache.reset_turn();
    }

    fn prefix_cache_stats(&self) -> crate::cache::PrefixCacheStats {
        self.prefix_cache.stats()
    }

    fn last_cache_hit(&self) -> Option<bool> {
        self.prefix_cache.last_check_was_hit()
    }

    fn estimated_cache_tokens(&self) -> usize {
        self.prefix_cache.last_cached_tokens()
    }

    fn prefix_hash_snapshot(&self) -> Option<u64> {
        self.prefix_cache.snapshot_hash()
    }
    fn prefix_hash_restore(&self, saved: Option<u64>) {
        self.prefix_cache.restore_hash(saved);
    }
}

// ── CloudClient trait ───────────────────────────────────────────────

/// Common interface for all inference providers (cloud + local).
#[async_trait]
pub trait CloudClient: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;
    async fn complete_stream(&self, req: CompletionRequest, tx: mpsc::Sender<String>)
    -> Result<()>;
    async fn complete_stream_structured(
        &self,
        req: CompletionRequest,
        tx: mpsc::Sender<StreamDelta>,
    ) -> Result<()> {
        let (legacy_tx, mut legacy_rx) = mpsc::channel::<String>(256);
        let forwarder = tokio::spawn(async move {
            while let Some(token) = legacy_rx.recv().await {
                let delta = if let Some(stripped) = token.strip_prefix("\x02REASONING\x02") {
                    StreamDelta::Reasoning(stripped.to_string())
                } else if let Some(stripped) = token.strip_prefix("\x00USAGE:") {
                    let parts: Vec<&str> = stripped.split(':').collect();
                    if parts.len() == 3 {
                        StreamDelta::Usage {
                            prompt_tokens: parts[0].parse().unwrap_or(0),
                            completion_tokens: parts[1].parse().unwrap_or(0),
                            cache_read_tokens: parts[2].parse().unwrap_or(0),
                            cache_write_tokens: 0,
                        }
                    } else {
                        continue;
                    }
                } else {
                    StreamDelta::Text(token)
                };
                if tx.send(delta).await.is_err() {
                    break;
                }
            }
        });
        self.complete_stream(req, legacy_tx).await?;
        forwarder.await.ok();
        Ok(())
    }
    fn provider(&self) -> ModelBackend;
    fn model_name(&self) -> &str;
    /// Reset per-turn prefix cache state. Default: no-op.
    fn prefix_cache_reset(&self) {}
    /// Return prefix cache stats. Default: empty stats.
    fn prefix_cache_stats(&self) -> crate::cache::PrefixCacheStats {
        crate::cache::PrefixCacheStats::default()
    }
    /// Whether the most recent prefix check was a cache hit (None = not checked).
    fn last_cache_hit(&self) -> Option<bool> {
        None
    }
    /// Estimated token count saved by prefix cache hit (0 = no hit or not tracked).
    fn estimated_cache_tokens(&self) -> usize {
        0
    }
    /// Snapshot prefix hash for save/restore around internal calls (e.g. summarization).
    fn prefix_hash_snapshot(&self) -> Option<u64> {
        None
    }
    /// Restore a previously-saved prefix hash.
    fn prefix_hash_restore(&self, _saved: Option<u64>) {}
}

// ── helpers ─────────────────────────────────────────────────────────

/// Parse tool call arguments from the LLM response.
/// Handles both the standard OpenAI string format ("{\"path\":\"...\"}")
/// and the object format used by some proxies/gateways ({"path":"..."}).
fn parse_tool_arguments(args: &serde_json::Value) -> serde_json::Value {
    // Standard OpenAI format: arguments is a JSON-encoded string
    if let Some(s) = args.as_str() {
        return serde_json::from_str(s).unwrap_or_default();
    }
    // Some gateways/proxies return arguments as a JSON object directly
    if args.is_object() {
        return args.clone();
    }
    serde_json::Value::Object(serde_json::Map::new())
}

fn lower_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|msg| {
            let role = msg.role.as_str();
            let mut obj = serde_json::json!({"role": role});

            // Tool result messages
            if role == "tool" {
                if let Some(ContentPart::ToolResult {
                    tool_call_id,
                    name,
                    result,
                }) = msg.content.first()
                {
                    // Skip malformed tool messages with empty tool_call_id
                    if tool_call_id.is_empty() {
                        obj["role"] = serde_json::json!("user");
                        obj["content"] = serde_json::json!(format!("[system note: a tool call failed — {}] {}", result, if name.is_empty() { "" } else { name }));
                        return obj;
                    }
                    obj["tool_call_id"] = serde_json::json!(tool_call_id);
                    obj["content"] = serde_json::json!(result);
                    if !name.is_empty() {
                        obj["name"] = serde_json::json!(name);
                    }
                }
                return obj;
            }

            // Check for image parts — build multipart content if present
            let has_images = msg.content.iter().any(|p| matches!(p, ContentPart::Image { .. }));
            if has_images {
                let mut parts: Vec<serde_json::Value> = Vec::new();
                let mut tc_vals: Vec<serde_json::Value> = Vec::new();
                for p in &msg.content {
                    match p {
                        ContentPart::Text { text } => {
                            parts.push(serde_json::json!({"type": "text", "text": text}));
                        }
                        ContentPart::Image { source_type: _, media_type, data } => {
                            parts.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": { "url": format!("data:{};base64,{}", media_type, data) }
                            }));
                        }
                        ContentPart::ImageRef { .. } => {
                            // ImageRef should be stripped by agent loop before reaching here.
                        }
                        ContentPart::ToolCall { id, name, arguments } => {
                            tc_vals.push(serde_json::json!({
                                "id": id, "type": "function",
                                "function": {"name": name, "arguments": serde_json::to_string(arguments).unwrap_or_default()},
                            }));
                        }
                        ContentPart::ToolResult { tool_call_id: _, name: _, result } => {
                            parts.push(serde_json::json!({"type": "text", "text": result}));
                        }
                        ContentPart::Thinking { text } => {
                            parts.push(serde_json::json!({"type": "text", "text": format!("[reasoning]\n{}", text)}));
                        }
                    }
                }
                obj["content"] = serde_json::json!(parts);
                if !tc_vals.is_empty() {
                    obj["tool_calls"] = serde_json::json!(tc_vals);
                }
                return obj;
            }

            // Separate text and tool-call parts
            let texts: Vec<&str> = msg
                .content
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            let thinking_texts: Vec<String> = msg.content.iter().filter_map(|p| match p {
                ContentPart::Thinking { text } => Some(format!("[reasoning]\n{}", text)),
                _ => None,
            }).collect();
            let tc_vals: Vec<serde_json::Value> = msg
                .content
                .iter()
                .filter_map(|p| match p {
                    ContentPart::ToolCall { id, name, arguments } => Some(serde_json::json!({
                        "id": id,
                        "type": "function",
                        "function": {"name": name, "arguments": serde_json::to_string(arguments).unwrap_or_default()},
                    })),
                    _ => None,
                })
                .collect();

            let all_texts: Vec<&str> = texts.iter().map(|s| *s)
                .chain(thinking_texts.iter().map(|s| s.as_str()))
                .collect();
            if all_texts.is_empty() && tc_vals.is_empty() {
                obj["content"] = serde_json::json!("");
            } else if !all_texts.is_empty() {
                obj["content"] = serde_json::json!(all_texts.join("\n"));
                if !tc_vals.is_empty() {
                    obj["tool_calls"] = serde_json::json!(tc_vals);
                }
            } else {
                obj["content"] = serde_json::Value::Null;
                obj["tool_calls"] = serde_json::json!(tc_vals);
            }
            obj
        })
        .collect()
}

fn build_tools_json(tools: &[loom_types::ToolDefinition]) -> serde_json::Value {
    serde_json::json!(
        tools
            .iter()
            .map(|t| serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                },
            }))
            .collect::<Vec<_>>()
    )
}

// ── tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_gpu_does_not_panic() {
        let info = InferenceEngine::detect_gpu();
        assert!(!info.vendor.is_empty() || !info.supported);
    }

    #[test]
    fn test_connect_blocking_does_not_crash() {
        // Should return Ok even if endpoint is unreachable during test
        let result = InferenceEngine::connect_blocking("http://localhost:1", "test-model", 4096);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dummy_engine_builds() {
        let engine = InferenceEngine::dummy();
        assert_eq!(engine.model_name(), "dummy");
        assert_eq!(engine.provider(), ModelBackend::LmStudio);
    }
}
