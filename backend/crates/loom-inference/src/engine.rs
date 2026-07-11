//! InferenceEngine — local inference via OpenAI-compatible HTTP API (LM Studio / Ollama)
//! and CloudClient trait for provider dispatch.

use crate::cache::{CacheStatus, PrefixCache};
use anyhow::Result;
use async_trait::async_trait;
use loom_context::PrefixDigest;
use loom_types::{
    CompletionRequest, CompletionResponse, ContentPart, GpuInfo, Message, ModelBackend,
    StreamDelta, ToolCall,
};
use reqwest::Client as HttpClient;
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc;

/// Global proxy URL and enabled flag, synced from ToolPrefsConfig when saved/loaded.
static GLOBAL_PROXY: OnceLock<Mutex<(Option<String>, bool)>> = OnceLock::new();

/// Update the global proxy from ToolPrefsConfig.
/// Call this from save_tool_prefs so all HTTP clients pick it up.
pub fn set_global_proxy(url: Option<String>, enabled: bool) {
    let lock = GLOBAL_PROXY.get_or_init(|| Mutex::new((None, true)));
    if let Ok(mut guard) = lock.lock() {
        *guard = (url.filter(|s| !s.is_empty()), enabled);
    }
}

/// Resolve the effective proxy URL: global setting first, then env vars.
/// Respects proxy_enabled flag — when false, returns None (direct connection).
fn get_effective_proxy() -> Option<String> {
    // Check global proxy config (set via tool_prefs)
    if let Some(lock) = GLOBAL_PROXY.get() {
        if let Ok(guard) = lock.lock() {
            let (url, enabled) = &*guard;
            if !enabled {
                return None; // proxy explicitly disabled
            }
            if let Some(url) = url {
                if !url.is_empty() {
                    return Some(url.clone());
                }
            }
        }
    }
    // Fall through to environment variables
    std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("http_proxy"))
        .ok()
        .filter(|s| !s.is_empty())
}

/// 构建 HTTP 客户端（带 UA，给 web_search/web_fetch 用），支持全局代理
pub fn build_http_client_with_ua() -> HttpClient {
    let mut builder = HttpClient::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (compatible; openLoom/1.0; +https://github.com/godsir/openloom)");

    if let Some(url) = get_effective_proxy() {
        if let Ok(proxy) = build_proxy(&url) {
            builder = builder.proxy(proxy);
        } else {
            tracing::warn!(%url, "invalid proxy URL, connections will go direct");
        }
    }

    builder.build().unwrap_or_default()
}

/// 构建统一 HTTP 客户端（无 UA，给 API 调用用），支持全局代理
pub fn build_http_client() -> HttpClient {
    let mut builder = HttpClient::builder().connect_timeout(std::time::Duration::from_secs(10));

    if let Some(url) = get_effective_proxy() {
        if let Ok(proxy) = build_proxy(&url) {
            builder = builder.proxy(proxy);
        } else {
            tracing::warn!(%url, "invalid proxy URL, connections will go direct");
        }
    }

    builder.build().unwrap_or_default()
}

/// Build a reqwest::Proxy that routes through `proxy_url` but bypasses
/// localhost/loopback and any hosts listed in NO_PROXY / no_proxy.
fn build_proxy(proxy_url: &str) -> Result<reqwest::Proxy, reqwest::Error> {
    let url_owned = proxy_url.to_string();
    let mut proxy = reqwest::Proxy::custom(move |url| {
        let host = url.host_str().unwrap_or("");
        // Always bypass localhost and loopback
        if host == "localhost" || host == "127.0.0.1" || host == "::1" {
            return None; // direct connection
        }
        // Respect NO_PROXY / no_proxy env var
        if host_matches_no_proxy(host) {
            return None;
        }
        Some(url_owned.clone())
    });
    // Also set as default for HTTPS → HTTP upgrade detection
    proxy = proxy.no_proxy(reqwest::NoProxy::from_string("localhost,127.0.0.1,::1"));
    Ok(proxy)
}

/// Check if a host matches any entry in the NO_PROXY / no_proxy env var.
fn host_matches_no_proxy(host: &str) -> bool {
    let no_proxy = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .unwrap_or_default();
    if no_proxy.is_empty() {
        return false;
    }
    let host_lower = host.to_lowercase();
    no_proxy.split(',').any(|entry| {
        let entry = entry.trim().to_lowercase();
        if entry.is_empty() {
            return false;
        }
        // Exact match or suffix match (e.g. ".corp.com" matches "foo.corp.com")
        entry == host_lower
            || (entry.starts_with('.') && host_lower.ends_with(&entry))
            || host_lower.ends_with(&format!(".{}", entry))
    })
}

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
    /// Pending prefix digest for the next request (set by agent loop via set_prefix_digest).
    pending_digest: std::sync::Mutex<Option<PrefixDigest>>,
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
        let http = build_http_client();
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
                    .map(|a| {
                        a.iter()
                            .any(|m| m.get("id").and_then(|id| id.as_str()) == Some(model))
                    })
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
            pending_digest: std::sync::Mutex::new(None),
        })
    }

    /// Build an engine pointed at a known endpoint (no load trigger).
    pub fn new(base_url: String, model: String) -> Self {
        let http = build_http_client();
        Self {
            base_url,
            model,
            http,
            prefix_cache: PrefixCache::new(2),
            pending_digest: std::sync::Mutex::new(None),
        }
    }

    /// Dummy engine for tests — will fail at call time.
    pub fn dummy() -> Self {
        Self {
            base_url: "http://localhost:1".into(),
            model: "dummy".into(),
            http: HttpClient::new(),
            prefix_cache: PrefixCache::new(2),
            pending_digest: std::sync::Mutex::new(None),
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
        let digest = self.pending_digest.lock().unwrap().clone();
        let (cache_status, _, _reasons) = self.prefix_cache.check_digest(&digest);
        match cache_status {
            CacheStatus::Hit => tracing::info!("KV cache hit -- llama.cpp reuses prefix"),
            CacheStatus::ColdStart => tracing::info!("KV cache cold start -- local model"),
            _ => tracing::info!("KV cache miss -- local model"),
        }

        let max_tokens = if req.max_tokens > 0 {
            req.max_tokens
        } else {
            4096
        };
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "messages": messages,
        });
        if !req.stop.is_empty() {
            body["stop"] = serde_json::json!(req.stop);
        }
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

        // Surface truncation: finish_reason="length" means the local model hit
        // the token ceiling. CompletionResponse has no field for it, so warn.
        if matches!(json["choices"][0]["finish_reason"].as_str(), Some("length")) {
            tracing::warn!(
                model = %self.model,
                completion_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0),
                "completion truncated: finish_reason=length (hit max_tokens)"
            );
        }

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
        let digest = self.pending_digest.lock().unwrap().clone();
        let (cache_status, _, _reasons) = self.prefix_cache.check_digest(&digest);
        match cache_status {
            CacheStatus::Hit => tracing::info!("KV cache hit (stream)"),
            CacheStatus::ColdStart => tracing::info!("KV cache cold start (stream)"),
            _ => tracing::info!("KV cache miss (stream)"),
        }
        let max_tokens = if req.max_tokens > 0 {
            req.max_tokens
        } else {
            4096
        };
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": max_tokens,
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
        let idle_dur = std::time::Duration::from_secs(120);
        loop {
            // Idle timeout: a stalled mid-stream connection errors out instead
            // of hanging forever.
            let done = match tokio::time::timeout(idle_dur, stream.next()).await {
                Err(_) => return Err(anyhow::anyhow!("stream idle timeout")),
                Ok(Some(chunk)) => {
                    buf.extend_from_slice(&chunk?);
                    false
                }
                Ok(None) => {
                    // Flush residual: append a synthetic frame boundary so a
                    // terminal (blank-line-less) frame is parsed once more.
                    if buf.is_empty() {
                        break;
                    }
                    buf.extend_from_slice(b"\n\n");
                    true
                }
            };
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
                                    .send(format!("\x02REASONING\x02{r}"))
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
                                        "\x00USAGE:{}:{}:{}:{}",
                                        u["prompt_tokens"].as_u64().unwrap_or(0),
                                        u["completion_tokens"].as_u64().unwrap_or(0),
                                        u["prompt_tokens_details"]["cached_tokens"]
                                            .as_u64()
                                            .unwrap_or(0),
                                        u["prompt_tokens_details"]["cache_creation_input_tokens"]
                                            .as_u64()
                                            .unwrap_or(0),
                                    ))
                                    .await;
                            }
                        }
                    }
                }
            }
            if done {
                break;
            }
        }
        Ok(())
    }

    /// Token count using the Cl100k tokenizer (real BPE, not char/4 heuristic).
    pub fn token_count(&self, text: &str) -> usize {
        loom_context::bpe().encode_with_special_tokens(text).len()
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
    match build_http_client()
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

/// Classified error returned from a single non-streaming completion attempt so
/// the retry layer can tell retryable failures (429/5xx, transport) from
/// permanent ones (400/401/403/404/413/422, parse errors).
///
/// `status` carries the HTTP status code when the failure was an HTTP error
/// response; `retry_after` carries the parsed `Retry-After` header (in seconds)
/// when present. `transient` only governs status-less failures: it is `true`
/// for connect/timeout/transport errors (worth retrying) and `false` for
/// deterministic local failures such as JSON parse errors (not worth retrying).
pub(crate) struct RetryableError {
    pub status: Option<u16>,
    pub retry_after: Option<std::time::Duration>,
    pub transient: bool,
    pub source: anyhow::Error,
}

impl RetryableError {
    /// Build an error from a non-success HTTP response: classify by status code
    /// and capture any `Retry-After` header.
    pub fn from_status(
        status: u16,
        retry_after: Option<std::time::Duration>,
        source: anyhow::Error,
    ) -> Self {
        Self {
            status: Some(status),
            retry_after,
            transient: false,
            source,
        }
    }

    /// Build an error from a transport/connect/timeout failure (no HTTP status
    /// was ever received). Always retryable.
    pub fn transport(source: anyhow::Error) -> Self {
        Self {
            status: None,
            retry_after: None,
            transient: true,
            source,
        }
    }

    /// Whether this failure should be retried.
    ///
    /// Retry on 429 and gateway/server errors (500/502/503/504) and on
    /// status-less transport errors. Do NOT retry permanent client errors
    /// (400/401/403/404/413/422, etc.) or deterministic local failures.
    pub fn is_retryable(&self) -> bool {
        match self.status {
            Some(code) => matches!(code, 429 | 500 | 502 | 503 | 504),
            None => self.transient,
        }
    }
}

impl From<RetryableError> for anyhow::Error {
    fn from(e: RetryableError) -> Self {
        e.source
    }
}

/// Any plain `anyhow::Error` propagated via `?` inside a completion attempt
/// (lock poisoning, body read after a 2xx, JSON parse) is a deterministic local
/// failure: classify it as non-retryable with no HTTP status.
impl From<anyhow::Error> for RetryableError {
    fn from(source: anyhow::Error) -> Self {
        Self {
            status: None,
            retry_after: None,
            transient: false,
            source,
        }
    }
}

/// Parse the `Retry-After` response header. Anthropic/OpenAI send it as an
/// integer number of seconds on 429 (and sometimes 503) responses.
pub(crate) fn parse_retry_after(
    headers: &reqwest::header::HeaderMap,
) -> Option<std::time::Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
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
        let digest = self.pending_digest.lock().unwrap().clone();
        let (cache_status, _, _reasons) = self.prefix_cache.check_digest(&digest);
        match cache_status {
            CacheStatus::Hit => tracing::info!("KV cache hit (structured stream)"),
            CacheStatus::ColdStart => tracing::info!("KV cache cold start (structured stream)"),
            _ => tracing::info!("KV cache miss (structured stream)"),
        }

        let max_tokens = if req.max_tokens > 0 {
            req.max_tokens
        } else {
            4096
        };
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": max_tokens,
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
        let idle_dur = std::time::Duration::from_secs(120);
        loop {
            let done = match tokio::time::timeout(idle_dur, stream.next()).await {
                Err(_) => return Err(anyhow::anyhow!("stream idle timeout")),
                Ok(Some(chunk)) => {
                    buf.extend_from_slice(&chunk?);
                    false
                }
                Ok(None) => {
                    // Flush residual: append a synthetic frame boundary so a
                    // terminal (blank-line-less) frame is parsed once more.
                    if buf.is_empty() {
                        break;
                    }
                    buf.extend_from_slice(b"\n\n");
                    true
                }
            };
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
                                        && !id.is_empty()
                                        && !name.is_empty()
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
                                        cache_write_tokens:
                                            u["prompt_tokens_details"]["cache_creation_input_tokens"]
                                                .as_u64()
                                                .unwrap_or(0),
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
            if done {
                break;
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
    fn set_prefix_digest(&self, digest: Option<PrefixDigest>) {
        *self.pending_digest.lock().unwrap() = digest.clone();
        self.prefix_cache.check_digest(&digest);
    }
    fn prefix_digest_snapshot(&self) -> Option<PrefixDigest> {
        self.prefix_cache.snapshot_digest()
    }
    fn prefix_digest_restore(&self, saved: Option<PrefixDigest>) {
        self.prefix_cache.restore_digest(saved);
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
                    if parts.len() >= 3 {
                        StreamDelta::Usage {
                            prompt_tokens: parts[0].parse().unwrap_or(0),
                            completion_tokens: parts[1].parse().unwrap_or(0),
                            cache_read_tokens: parts[2].parse().unwrap_or(0),
                            cache_write_tokens: parts
                                .get(3)
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0),
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
    /// Set the prefix digest for the next request. Default: no-op.
    fn set_prefix_digest(&self, _digest: Option<PrefixDigest>) {}
    /// Snapshot prefix digest for save/restore around internal calls. Default: no-op.
    fn prefix_digest_snapshot(&self) -> Option<PrefixDigest> {
        None
    }
    /// Restore a previously-saved prefix digest. Default: no-op.
    fn prefix_digest_restore(&self, _saved: Option<PrefixDigest>) {}
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

            let all_texts: Vec<&str> = texts.iter().copied()
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

    #[test]
    fn test_retryable_error_status_classification() {
        // Retryable: 429 + gateway/server 5xx.
        for code in [429u16, 500, 502, 503, 504] {
            let e = RetryableError::from_status(code, None, anyhow::anyhow!("boom"));
            assert!(e.is_retryable(), "status {code} should be retryable");
        }
        // Permanent client errors must NOT retry.
        for code in [400u16, 401, 403, 404, 413, 422] {
            let e = RetryableError::from_status(code, None, anyhow::anyhow!("boom"));
            assert!(!e.is_retryable(), "status {code} should not be retryable");
        }
    }

    #[test]
    fn test_retryable_error_transport_is_retryable() {
        let e = RetryableError::transport(anyhow::anyhow!("connection reset"));
        assert!(e.is_retryable());
        assert!(e.status.is_none());
    }

    #[test]
    fn test_anyhow_into_retryable_is_not_retryable() {
        // A plain anyhow error (parse failure, lock poisoning) propagated via `?`
        // is a deterministic local failure and must not be retried.
        let e: RetryableError = anyhow::anyhow!("parse error").into();
        assert!(!e.is_retryable());
        assert!(e.status.is_none());
    }

    #[test]
    fn test_parse_retry_after_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("30"),
        );
        assert_eq!(
            parse_retry_after(&headers),
            Some(std::time::Duration::from_secs(30))
        );
    }

    #[test]
    fn test_parse_retry_after_missing_or_nonnumeric() {
        let empty = reqwest::header::HeaderMap::new();
        assert_eq!(parse_retry_after(&empty), None);

        // HTTP-date form is not integer seconds; we intentionally ignore it
        // (Anthropic/OpenAI send integer seconds) and fall back to backoff.
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("Wed, 21 Oct 2025 07:28:00 GMT"),
        );
        assert_eq!(parse_retry_after(&headers), None);
    }
}
