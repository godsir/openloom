//! Anthropic Messages API client.

use anyhow::Result;
use async_trait::async_trait;
use loom_context::PrefixDigest;
use loom_types::{
    CompletionRequest, CompletionResponse, ContentPart, Message, ModelBackend, StreamDelta,
    ToolCall, ToolChoice,
};
use reqwest::Client as HttpClient;
use tokio::sync::mpsc;

use crate::cache::{CacheStatus, PrefixCache};
use crate::engine::CloudClient;
use crate::engine::{RetryableError, parse_retry_after};

/// Conservative fallback for `max_tokens` when the request leaves it unset
/// (`req.max_tokens == 0`).
///
/// The previous hardcoded default (128000) exceeds the output cap of many
/// Claude models, producing a hard 400 that then burns retries. 16384 fits
/// comfortably within every current Claude model's `max_tokens` ceiling while
/// still allowing long completions. Callers that know the model's true cap
/// (via `ModelConfig::effective_max_output()`) should pass it in `max_tokens`;
/// `AnthropicClient` does not carry the full `ModelConfig`, so it cannot derive
/// the exact cap itself.
const DEFAULT_MAX_TOKENS: u64 = 16384;

/// Extra `max_tokens` headroom reserved for the visible response when extended
/// thinking is enabled and the caller's `max_tokens` would not exceed the
/// thinking budget (Anthropic requires `max_tokens > budget_tokens`).
const DEFAULT_RESPONSE_HEADROOM: u64 = 4096;

pub struct AnthropicClient {
    api_key: String,
    model: String,
    base_url: String,
    http: HttpClient,
    prefix_cache: PrefixCache,
    /// Pending prefix digest for the next request (set by agent loop via set_prefix_digest).
    pending_digest: std::sync::Mutex<Option<PrefixDigest>>,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String, base_url: String) -> Self {
        let http = HttpClient::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        // Normalize base_url: strip any /v1 suffix so we don't double it when appending /v1/messages.
        // e.g. "https://api.deepseek.com/v1" -> "https://api.deepseek.com"
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        let base_url = if base_url.ends_with("/v1") {
            base_url[..base_url.len() - 3].to_string()
        } else {
            base_url
        };
        Self {
            api_key,
            model,
            base_url,
            http,
            prefix_cache: PrefixCache::new(2),
            pending_digest: std::sync::Mutex::new(None),
        }
    }

    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url)
    }

    /// Resolve the request body's `max_tokens` and (optional) `thinking` block.
    ///
    /// `req.max_tokens == 0` means "unset"; we fall back to a conservative
    /// [`DEFAULT_MAX_TOKENS`] rather than a large hardcoded value that exceeds
    /// many models' output caps (which the API rejects with a 400). The
    /// `AnthropicClient` does not hold the full `ModelConfig`, so it cannot read
    /// `ModelConfig::effective_max_output()`; callers that know the model's cap
    /// should pass it via `req.max_tokens`.
    ///
    /// When extended thinking is enabled, Anthropic requires
    /// `budget_tokens >= 1024` and `max_tokens > budget_tokens` (the thinking
    /// budget is drawn from the `max_tokens` allotment). We floor the budget at
    /// 1024 and then raise `max_tokens` above it if necessary, so the request is
    /// never self-contradictory.
    fn resolve_tokens(&self, req: &CompletionRequest) -> (u64, Option<serde_json::Value>) {
        let mut max_tokens: u64 = if req.max_tokens > 0 {
            req.max_tokens as u64
        } else {
            DEFAULT_MAX_TOKENS
        };
        let thinking = match req.thinking_budget {
            Some(budget) if budget > 0 => {
                // Anthropic requires budget_tokens >= 1024.
                let budget = (budget as u64).max(1024);
                // max_tokens must leave room beyond the thinking budget.
                if max_tokens <= budget {
                    max_tokens = budget + DEFAULT_RESPONSE_HEADROOM;
                }
                Some(serde_json::json!({"type": "enabled", "budget_tokens": budget}))
            }
            // budget == 0 or None → extended thinking off.
            _ => None,
        };
        (max_tokens, thinking)
    }

    async fn complete_with_retry(
        &self,
        req: &CompletionRequest,
        retries: usize,
    ) -> Result<CompletionResponse> {
        let mut last_err = None;
        for attempt in 0..=retries {
            match self.try_complete(req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    tracing::warn!(
                        attempt,
                        status = ?e.status,
                        retryable = e.is_retryable(),
                        error = %e.source,
                        "Anthropic API call failed"
                    );
                    if !e.is_retryable() || attempt == retries {
                        // Permanent failure (e.g. 401/400) or attempts exhausted:
                        // surface immediately instead of burning more retries.
                        return Err(e.into());
                    }
                    // Honor Retry-After (seconds) when the server provided one
                    // (typically on 429); otherwise fall back to exponential backoff.
                    let delay = e.retry_after.unwrap_or_else(|| {
                        std::time::Duration::from_millis(2u64.pow((attempt + 1) as u32) * 500)
                    });
                    last_err = Some(e);
                    tokio::time::sleep(delay).await;
                }
            }
        }
        Err(last_err
            .map(anyhow::Error::from)
            .unwrap_or_else(|| anyhow::anyhow!("no completion attempts were made")))
    }

    async fn try_complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, RetryableError> {
        let eff = req.effective_messages();
        let digest = self.pending_digest.lock().unwrap().clone();
        let (cache_status, _, reasons) = self.prefix_cache.check_digest(&digest);
        match cache_status {
            CacheStatus::Hit => tracing::info!("KV cache hit -- prefix unchanged"),
            CacheStatus::BreakingMiss => {
                tracing::info!(?reasons, "KV cache miss (breaking) -- prefix changed");
            }
            CacheStatus::AdditiveMiss => {
                tracing::info!("KV cache miss (additive) -- prefix unchanged, suffix grew");
            }
            CacheStatus::ColdStart => {
                tracing::info!("KV cache cold start -- first request in sequence");
            }
        }
        let (system_prompt, messages) = self.lower_messages(&eff, &digest, cache_status);
        let (max_tokens, thinking) = self.resolve_tokens(req);
        let mut body = serde_json::json!({"model": self.model, "max_tokens": max_tokens, "temperature": req.temperature, "messages": messages});
        if !req.stop.is_empty() {
            body["stop_sequences"] = serde_json::json!(req.stop);
        }
        if let Some(sys) = system_prompt {
            body["system"] = sys.clone();
        }
        if !req.tools.is_empty() {
            let tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
                serde_json::json!({"name": t.name, "description": t.description, "input_schema": t.input_schema})
            }).collect();
            body["tools"] = serde_json::json!(tools);
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
        if let Some(thinking) = thinking {
            body["thinking"] = thinking;
        }

        let url = self.messages_url();
        let resp = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| RetryableError::transport(anyhow::Error::new(e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let retry_after = parse_retry_after(resp.headers());
            let text = resp.text().await.unwrap_or_default();
            return Err(RetryableError::from_status(
                status.as_u16(),
                retry_after,
                anyhow::anyhow!("Anthropic API error {status} {url}: {text}"),
            ));
        }

        let body_text = resp
            .text()
            .await
            .map_err(|e| RetryableError::transport(anyhow::Error::new(e)))?;
        let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            anyhow::anyhow!(
                "Anthropic response parse error: {e}, body: {}",
                truncate(&body_text, 500)
            )
        })?;

        let (text, tool_calls, thinking) = self.parse_content(&json);
        let prompt_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize;
        let cached_tokens = json["usage"]["cache_read_input_tokens"]
            .as_u64()
            .unwrap_or(0) as usize;

        // Surface truncation: Anthropic sets stop_reason="max_tokens" when the
        // response hit the token ceiling (vs "end_turn"/"tool_use"). There is
        // no response field for it, so warn — a truncated reply otherwise looks
        // complete to callers.
        if matches!(json["stop_reason"].as_str(), Some("max_tokens")) {
            tracing::warn!(
                model = %self.model,
                completion_tokens,
                "completion truncated: stop_reason=max_tokens"
            );
        }

        Ok(CompletionResponse {
            text,
            tool_calls,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            latency_ms: 0,
            thinking,
            images: Vec::new(),
        })
    }

    fn lower_messages(
        &self,
        messages: &[Message],
        _digest: &Option<PrefixDigest>,
        cache_status: CacheStatus,
    ) -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
        let system_text = messages
            .iter()
            .filter(|m| m.role.as_str() == "system")
            .filter_map(|m| m.content.first())
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let cache_hit = matches!(cache_status, CacheStatus::Hit);
        let system_prompt: Option<serde_json::Value> = if system_text.is_empty() {
            None
        } else {
            if cache_hit {
                Some(serde_json::json!([{
                    "type": "text",
                    "text": system_text,
                    "cache_control": { "type": "ephemeral" }
                }]))
            } else {
                Some(serde_json::json!(system_text))
            }
        };
        let msgs: Vec<serde_json::Value> = messages.iter()
            .filter(|m| m.role.as_str() != "system")
            .enumerate().map(|(i, msg)| {
            // Anthropic Messages API only supports "user" and "assistant" roles.
            // Tool results must be wrapped in user messages (not a separate "tool" role).
            let anthropic_role = match msg.role.as_str() {
                "tool" => "user",
                other => other,
            };
            let content: Vec<serde_json::Value> = msg.content.iter().map(|part| match part {
                ContentPart::Text { text } => serde_json::json!({"type": "text", "text": text}),
                ContentPart::Image { source_type, media_type, data } => serde_json::json!({
                    "type": "image", "source": {"type": source_type, "media_type": media_type, "data": data}
                }),
                ContentPart::ToolCall { id, name, arguments } => serde_json::json!({
                    "type": "tool_use", "id": id, "name": name, "input": arguments
                }),
                ContentPart::ToolResult { tool_call_id, name: _, result } => serde_json::json!({
                    "type": "tool_result", "tool_use_id": tool_call_id, "content": result
                }),
                ContentPart::Thinking { text } => serde_json::json!({
                    "type": "thinking", "thinking": text
                }),
                ContentPart::ImageRef { .. } => {
                    tracing::warn!("ImageRef leaked to inference layer (anthropic), skipping");
                    serde_json::json!({"type": "text", "text": "[image omitted]"})
                }
            }).collect();
            let mut msg_json = serde_json::json!({"role": anthropic_role, "content": content});
            if cache_hit && i == 0 {
                if let Some(content_arr) = msg_json
                    .get_mut("content")
                    .and_then(|c| c.as_array_mut())
                    .and_then(|arr| arr.last_mut())
                {
                    content_arr["cache_control"] =
                        serde_json::json!({ "type": "ephemeral" });
                }
            }
            msg_json
        }).collect();
        (system_prompt, msgs)
    }

    fn parse_content(&self, json: &serde_json::Value) -> (String, Vec<ToolCall>, Option<String>) {
        let content = json["content"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
        let text: String = content
            .iter()
            .filter_map(|b| {
                if b["type"] == "text" {
                    b["text"].as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let tool_calls: Vec<ToolCall> = content
            .iter()
            .filter(|b| b["type"] == "tool_use")
            .filter_map(|b| {
                Some(ToolCall {
                    id: b["id"].as_str()?.to_string(),
                    name: b["name"].as_str()?.to_string(),
                    arguments: b["input"].clone(),
                })
            })
            .collect();
        let thinking: Option<String> = {
            let texts: Vec<String> = content
                .iter()
                .filter_map(|b| {
                    if b["type"] == "thinking" {
                        b["thinking"].as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        };
        (text, tool_calls, thinking)
    }
}

fn truncate(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map(|(i, _)| &s[..i]).unwrap_or(s)
}

#[async_trait]
impl CloudClient for AnthropicClient {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.complete_with_retry(&req, 3).await
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        let eff = req.effective_messages();
        let digest = self.pending_digest.lock().unwrap().clone();
        let (cache_status, _, _reasons) = self.prefix_cache.check_digest(&digest);
        match cache_status {
            CacheStatus::Hit => tracing::info!("KV cache hit (stream)"),
            CacheStatus::BreakingMiss => tracing::info!("KV cache miss (stream -- breaking)"),
            CacheStatus::AdditiveMiss => tracing::info!("KV cache miss (stream -- additive)"),
            CacheStatus::ColdStart => tracing::info!("KV cache cold start (stream)"),
        }
        let (system_prompt, messages) = self.lower_messages(&eff, &digest, cache_status);
        let (max_tokens, thinking) = self.resolve_tokens(&req);
        let mut body = serde_json::json!({"model": self.model, "max_tokens": max_tokens, "temperature": req.temperature, "messages": messages, "stream": true});
        if let Some(sys) = system_prompt {
            body["system"] = sys.clone();
        }
        if let Some(thinking) = thinking {
            body["thinking"] = thinking;
        }
        if !req.tools.is_empty() {
            let tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
                serde_json::json!({"name": t.name, "description": t.description, "input_schema": t.input_schema})
            }).collect();
            body["tools"] = serde_json::json!(tools);
        }

        let url = self.messages_url();
        let resp = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Anthropic API error {} {}: {}",
                resp.status(),
                url,
                resp.text().await.unwrap_or_default()
            );
        }

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        let mut prompt_tokens: u64 = 0;
        let mut completion_tokens: u64 = 0;
        let mut cached_tokens: u64 = 0;
        let idle_dur = std::time::Duration::from_secs(120);

        loop {
            // Wrap each poll in an idle timeout so a stalled mid-stream
            // connection errors out instead of hanging forever.
            let done = match tokio::time::timeout(idle_dur, stream.next()).await {
                Err(_) => return Err(anyhow::anyhow!("stream idle timeout")),
                Ok(Some(chunk_result)) => {
                    buffer.extend_from_slice(&chunk_result?);
                    false
                }
                Ok(None) => {
                    // Stream ended. Flush any residual bytes by appending a
                    // synthetic frame boundary so the in-loop parser runs once
                    // more on the terminal (blank-line-less) frame, then exit.
                    if buffer.is_empty() {
                        break;
                    }
                    buffer.extend_from_slice(b"\n\n");
                    true
                }
            };
            // Find complete SSE frames (delimited by \n\n) without splitting UTF-8
            while let Some(pos) = buffer.windows(2).position(|w| w == b"\n\n") {
                let frame_bytes = buffer[..pos].to_vec();
                buffer.drain(..pos + 2);
                let frame = String::from_utf8_lossy(&frame_bytes);
                for line in frame.lines() {
                    if let Some(data) = line.strip_prefix("data: ")
                        && let Ok(val) = serde_json::from_str::<serde_json::Value>(data)
                    {
                        if let Some(text) = val["delta"]["text"].as_str()
                            && tx.send(text.to_string()).await.is_err()
                        {
                            return Ok(());
                        }
                        if let Some(usage) = val.get("message").and_then(|m| m.get("usage")) {
                            prompt_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
                            cached_tokens = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
                        }
                        if let Some(usage) = val.get("usage") {
                            completion_tokens =
                                usage["output_tokens"].as_u64().unwrap_or(completion_tokens);
                        }
                    }
                }
            }
            if done {
                break;
            }
        }
        if prompt_tokens > 0 || completion_tokens > 0 {
            let _ = tx
                .send(format!(
                    "\x00USAGE:{prompt_tokens}:{completion_tokens}:{cached_tokens}"
                ))
                .await;
        }
        Ok(())
    }

    async fn complete_stream_structured(
        &self,
        req: CompletionRequest,
        tx: mpsc::Sender<StreamDelta>,
    ) -> Result<()> {
        use futures::StreamExt;

        let eff = req.effective_messages();
        let digest = self.pending_digest.lock().unwrap().clone();
        let (cache_status, _, _reasons) = self.prefix_cache.check_digest(&digest);
        match cache_status {
            CacheStatus::Hit => tracing::info!("KV cache hit (stream)"),
            CacheStatus::BreakingMiss => tracing::info!("KV cache miss (stream -- breaking)"),
            CacheStatus::AdditiveMiss => tracing::info!("KV cache miss (stream -- additive)"),
            CacheStatus::ColdStart => tracing::info!("KV cache cold start (stream)"),
        }
        let (system_prompt, messages) = self.lower_messages(&eff, &digest, cache_status);
        let (max_tokens, thinking) = self.resolve_tokens(&req);
        let mut body = serde_json::json!({"model": self.model, "max_tokens": max_tokens, "temperature": req.temperature, "messages": messages, "stream": true});
        if let Some(sys) = system_prompt {
            body["system"] = sys.clone();
        }
        if let Some(thinking) = thinking {
            body["thinking"] = thinking;
        }
        if !req.tools.is_empty() {
            let tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
                serde_json::json!({"name": t.name, "description": t.description, "input_schema": t.input_schema})
            }).collect();
            body["tools"] = serde_json::json!(tools);
        }

        let url = self.messages_url();
        let resp = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Anthropic API error {} {}: {}",
                resp.status(),
                url,
                resp.text().await.unwrap_or_default()
            );
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut active_tool_index: Option<usize> = None;
        let idle_dur = std::time::Duration::from_secs(120);

        loop {
            let done = match tokio::time::timeout(idle_dur, stream.next()).await {
                Err(_) => return Err(anyhow::anyhow!("stream idle timeout")),
                Ok(Some(chunk_result)) => {
                    buf.extend_from_slice(&chunk_result?);
                    false
                }
                Ok(None) => {
                    // Flush residual: append a synthetic frame boundary so the
                    // terminal (blank-line-less) frame is parsed once more.
                    if buf.is_empty() {
                        break;
                    }
                    buf.extend_from_slice(b"\n\n");
                    true
                }
            };
            while let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
                let frame_bytes = buf[..pos].to_vec();
                buf.drain(..pos + 2);
                for line_bytes in frame_bytes.split(|b| *b == b'\n') {
                    let line = String::from_utf8_lossy(line_bytes);
                    if let Some(data) = line.strip_prefix("data: ") {
                        let Ok(val) = serde_json::from_str::<serde_json::Value>(data) else {
                            continue;
                        };
                        match val["type"].as_str() {
                            Some("content_block_start")
                                if val["content_block"]["type"] == "tool_use" =>
                            {
                                let idx = val["index"].as_u64().unwrap_or(0) as usize;
                                let id = val["content_block"]["id"]
                                    .as_str()
                                    .unwrap_or("?")
                                    .to_string();
                                let name = val["content_block"]["name"]
                                    .as_str()
                                    .unwrap_or("?")
                                    .to_string();
                                active_tool_index = Some(idx);
                                let _ = tx
                                    .send(StreamDelta::ToolCallBegin {
                                        index: idx,
                                        id,
                                        name,
                                    })
                                    .await;
                            }
                            Some("content_block_delta") => match val["delta"]["type"].as_str() {
                                Some("text_delta") => {
                                    if let Some(text) = val["delta"]["text"].as_str()
                                        && tx
                                            .send(StreamDelta::Text(text.to_string()))
                                            .await
                                            .is_err()
                                    {
                                        return Ok(());
                                    }
                                }
                                Some("thinking_delta") => {
                                    if let Some(text) = val["delta"]["thinking"].as_str()
                                        && tx
                                            .send(StreamDelta::Reasoning(text.to_string()))
                                            .await
                                            .is_err()
                                    {
                                        return Ok(());
                                    }
                                }
                                Some("input_json_delta") => {
                                    if let Some(json) = val["delta"]["partial_json"].as_str() {
                                        let idx = active_tool_index.unwrap_or(0);
                                        let _ = tx
                                            .send(StreamDelta::ToolCallArgsChunk {
                                                index: idx,
                                                chunk: json.to_string(),
                                            })
                                            .await;
                                    }
                                }
                                _ => {}
                            },
                            Some("message_delta") => {
                                let u = &val["usage"];
                                // message_delta.usage only contains output_tokens;
                                // cache_creation_input_tokens lives in message_start.message.usage.
                                let _ = tx
                                    .send(StreamDelta::Usage {
                                        prompt_tokens: 0,
                                        completion_tokens: u["output_tokens"].as_u64().unwrap_or(0),
                                        cache_read_tokens: 0,
                                        cache_write_tokens: 0,
                                    })
                                    .await;
                            }
                            Some("message_start") => {
                                let u = &val["message"]["usage"];
                                let _ = tx
                                    .send(StreamDelta::Usage {
                                        prompt_tokens: u["input_tokens"].as_u64().unwrap_or(0),
                                        completion_tokens: 0,
                                        cache_read_tokens: u["cache_read_input_tokens"]
                                            .as_u64()
                                            .unwrap_or(0),
                                        cache_write_tokens: u["cache_creation_input_tokens"]
                                            .as_u64()
                                            .unwrap_or(0),
                                    })
                                    .await;
                            }
                            _ => {}
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
        ModelBackend::Anthropic
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
        *self.pending_digest.lock().unwrap() = digest;
    }

    fn prefix_digest_snapshot(&self) -> Option<PrefixDigest> {
        self.prefix_cache.snapshot_digest()
    }

    fn prefix_digest_restore(&self, saved: Option<PrefixDigest>) {
        self.prefix_cache.restore_digest(saved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
