//! OpenAI-compatible API client - OpenAI, DeepSeek, LM Studio, Ollama.

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{
    CompletionRequest, CompletionResponse, ContentPart, Message, ModelBackend,
    ModelConfig, StreamDelta, ToolCall, ToolChoice,
};
use reqwest::Client as HttpClient;

use crate::cache::PrefixCache;
use crate::engine::CloudClient;

pub struct OpenAIClient {
    api_key: String,
    model: String,
    base_url: String,
    http: HttpClient,
    prefix_cache: PrefixCache,
}

impl OpenAIClient {
    pub fn new(api_key: String, model: String, base_url: String, _is_local: bool) -> Self {
        Self { api_key, model, base_url, http: HttpClient::new(), prefix_cache: PrefixCache::new(2) }
    }

    async fn complete_with_retry(&self, req: &CompletionRequest, retries: usize) -> Result<CompletionResponse> {
        let mut last_err = None;
        for attempt in 0..=retries {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(2u64.pow(attempt as u32) * 500)).await;
            }
            match self.try_complete(req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => { tracing::warn!(attempt, error = %e, "API call failed"); last_err = Some(e); }
            }
        }
        Err(last_err.unwrap())
    }

    async fn try_complete(&self, req: &CompletionRequest) -> Result<CompletionResponse> {
        let eff = req.effective_messages();
        let (cache_hit, _) = self.prefix_cache.check(&eff);
        if cache_hit { tracing::info!("KV cache hit"); } else { tracing::info!("KV cache miss"); }
        let messages = self.lower_messages(&eff);
        let mut body = serde_json::json!({
            "model": self.model, "max_tokens": req.max_tokens, "messages": messages,
        });
        if !req.tools.is_empty() {
            body["tools"] = serde_json::json!(req.tools.iter().map(|t| serde_json::json!({
                "type": "function", "function": {
                    "name": t.name, "description": t.description, "parameters": t.input_schema,
                }
            })).collect::<Vec<_>>());
        }
        if let Some(ref tc) = req.tool_choice {
            body["tool_choice"] = match tc {
                ToolChoice::Auto => serde_json::json!("auto"),
                ToolChoice::None => serde_json::json!("none"),
                ToolChoice::Required => serde_json::json!("required"),
            };
        }
        if req.temperature > 0.0 { body["temperature"] = req.temperature.into(); }

        let resp = self.http.post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("API error {}: {}", resp.status(), resp.text().await.unwrap_or_default());
        }

        let body_text = resp.text().await?;
        let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            anyhow::anyhow!("API parse error: {}, body: {}", e, truncate(&body_text, 500))
        })?;
        let choice = &json["choices"][0]["message"];
        let raw_text = choice["content"].as_str().unwrap_or("").to_string();
        let mut tool_calls: Vec<ToolCall> = choice["tool_calls"].as_array()
            .map(|arr| arr.iter().filter_map(|tc| Some(ToolCall {
                id: tc["id"].as_str()?.to_string(),
                name: tc["function"]["name"].as_str()?.to_string(),
                arguments: serde_json::from_str(
                    tc["function"]["arguments"].as_str().unwrap_or("{}")
                ).unwrap_or_default(),
            })).collect()).unwrap_or_default();
        if tool_calls.is_empty() {
            tool_calls = parse_inline_tool_calls(&raw_text).1;
        }
        Ok(CompletionResponse {
            text: if tool_calls.is_empty() { raw_text } else { String::new() },
            tool_calls,
            prompt_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize,
            completion_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize,
            cached_tokens: json["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0) as usize,
            latency_ms: 0,
            thinking: choice["reasoning_content"].as_str().filter(|s| !s.is_empty()).map(String::from),
        })
    }

    fn lower_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
        messages.iter().map(|msg| {
            let role = msg.role.as_str();
            let mut obj = serde_json::json!({"role": role});
            if role == "assistant" { obj["reasoning_content"] = serde_json::json!(""); }
            if role == "tool" {
                if let Some(ContentPart::ToolResult { tool_call_id, name: _, result }) = msg.content.first() {
                    obj["tool_call_id"] = serde_json::json!(tool_call_id);
                    obj["content"] = serde_json::json!(result);
                }
                return obj;
            }
            let has_images = msg.content.iter().any(|p| matches!(p, ContentPart::Image { .. }));
            if has_images {
                let mut parts: Vec<serde_json::Value> = Vec::new();
                for p in &msg.content {
                    if let ContentPart::Text { text } = p {
                        parts.push(serde_json::json!({"type": "text", "text": text}));
                    }
                }
                for p in &msg.content {
                    if let ContentPart::Image { source_type: _, media_type, data } = p {
                        parts.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": {"url": format!("data:{};base64,{}", media_type, data)},
                        }));
                    }
                }
                obj["content"] = serde_json::json!(parts);
                return obj;
            }
            let texts: Vec<&str> = msg.content.iter()
                .filter_map(|p| match p { ContentPart::Text { text } => Some(text.as_str()), _ => None })
                .collect();
            let tc_vals: Vec<serde_json::Value> = msg.content.iter().filter_map(|p| match p {
                ContentPart::ToolCall { id, name, arguments } => Some(serde_json::json!({
                    "id": id, "type": "function",
                    "function": {"name": name, "arguments": serde_json::to_string(arguments).unwrap_or_default()},
                })),
                _ => None,
            }).collect();
            if texts.is_empty() && tc_vals.is_empty() {
                obj["content"] = serde_json::json!("");
            } else if !texts.is_empty() {
                obj["content"] = serde_json::json!(texts.join("\n"));
                if !tc_vals.is_empty() { obj["tool_calls"] = serde_json::json!(tc_vals); }
            } else {
                obj["content"] = serde_json::Value::Null;
                obj["tool_calls"] = serde_json::json!(tc_vals);
            }
            obj
        }).collect()
    }
}

#[async_trait]
impl CloudClient for OpenAIClient {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.complete_with_retry(&req, 3).await
    }

    async fn complete_stream(&self, req: CompletionRequest, tx: tokio::sync::mpsc::Sender<String>) -> Result<()> {
        let eff = req.effective_messages();
        let (cache_hit, _) = self.prefix_cache.check(&eff);
        if cache_hit { tracing::info!("KV cache hit (stream)"); } else { tracing::info!("KV cache miss (stream)"); }
        let messages = self.lower_messages(&eff);
        let mut body = serde_json::json!({
            "model": self.model, "max_tokens": req.max_tokens, "messages": messages,
            "stream": true, "stream_options": {"include_usage": true},
        });
        if req.temperature > 0.0 { body["temperature"] = req.temperature.into(); }
        if !req.tools.is_empty() {
            body["tools"] = serde_json::json!(req.tools.iter().map(|t| serde_json::json!({
                "type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema},
            })).collect::<Vec<_>>());
            body["tool_choice"] = serde_json::json!("auto");
        }
        let resp = self.http.post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key)).json(&body).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("API error {}: {}", resp.status(), resp.text().await.unwrap_or_default());
        }
        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
                let frame_bytes = buf[..pos].to_vec();
                buf.drain(..pos + 2);
                let frame = String::from_utf8_lossy(&frame_bytes);
                for line in frame.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" { return Ok(()); }
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                            let d = &val["choices"][0]["delta"];
                            if let Some(r) = d["reasoning_content"].as_str().filter(|s| !s.is_empty())
                                && tx.send(format!("\x02REASONING\x02{}", r)).await.is_err() { return Ok(()); }
                            if let Some(t) = d["content"].as_str()
                                && tx.send(t.to_string()).await.is_err() { return Ok(()); }
                            if let Some(u) = val.get("usage").filter(|u| u.is_object()) {
                                let _ = tx.send(format!("\x00USAGE:{}:{}:{}",
                                    u["prompt_tokens"].as_u64().unwrap_or(0),
                                    u["completion_tokens"].as_u64().unwrap_or(0),
                                    u["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0),
                                )).await;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn complete_stream_structured(&self, req: CompletionRequest, tx: tokio::sync::mpsc::Sender<StreamDelta>) -> Result<()> {
        let eff = req.effective_messages();
        let (cache_hit, _) = self.prefix_cache.check(&eff);
        if cache_hit { tracing::info!("KV cache hit (structured stream)"); } else { tracing::info!("KV cache miss (structured stream)"); }
        let messages = self.lower_messages(&eff);
        let mut body = serde_json::json!({
            "model": self.model, "max_tokens": req.max_tokens, "messages": messages,
            "stream": true, "stream_options": {"include_usage": true},
        });
        if req.temperature > 0.0 { body["temperature"] = req.temperature.into(); }
        if !req.tools.is_empty() {
            body["tools"] = serde_json::json!(req.tools.iter().map(|t| serde_json::json!({
                "type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema},
            })).collect::<Vec<_>>());
            body["tool_choice"] = serde_json::json!("auto");
        }
        let resp = self.http.post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key)).json(&body).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("API error {}: {}", resp.status(), resp.text().await.unwrap_or_default());
        }
        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
                let frame_bytes = buf[..pos].to_vec();
                buf.drain(..pos + 2);
                let frame = String::from_utf8_lossy(&frame_bytes);
                for line in frame.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" { return Ok(()); }
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                            let d = &val["choices"][0]["delta"];
                            if let Some(r) = d["reasoning_content"].as_str() && !r.is_empty()
                                && tx.send(StreamDelta::Reasoning(r.to_string())).await.is_err() { return Ok(()); }
                            if let Some(t) = d["content"].as_str() && !t.is_empty()
                                && tx.send(StreamDelta::Text(t.to_string())).await.is_err() { return Ok(()); }
                            if let Some(tcs) = d["tool_calls"].as_array() {
                                for tc in tcs {
                                    let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                                    if let (Some(id), Some(name)) = (tc["id"].as_str(), tc["function"]["name"].as_str())
                                        && tx.send(StreamDelta::ToolCallBegin { index: idx, id: id.to_string(), name: name.to_string() }).await.is_err() { return Ok(()); }
                                    if let Some(args) = tc["function"]["arguments"].as_str() && !args.is_empty()
                                        && tx.send(StreamDelta::ToolCallArgsChunk { index: idx, chunk: args.to_string() }).await.is_err() { return Ok(()); }
                                }
                            }
                            if let Some(u) = val.get("usage").filter(|u| u.is_object())
                                && tx.send(StreamDelta::Usage {
                                    prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0),
                                    completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0),
                                    cache_read_tokens: u["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0),
                                    cache_write_tokens: 0,
                                }).await.is_err() { return Ok(()); }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn provider(&self) -> ModelBackend { ModelBackend::OpenAI }
    fn model_name(&self) -> &str { &self.model }

    fn prefix_cache_reset(&self) { self.prefix_cache.reset_turn(); }
    fn prefix_cache_stats(&self) -> crate::cache::PrefixCacheStats { self.prefix_cache.stats() }
    fn last_cache_hit(&self) -> Option<bool> { self.prefix_cache.last_check_was_hit() }
    fn estimated_cache_tokens(&self) -> usize { self.prefix_cache.last_cached_tokens() }
    fn prefix_hash_snapshot(&self) -> Option<u64> { self.prefix_cache.snapshot_hash() }
    fn prefix_hash_restore(&self, saved: Option<u64>) { self.prefix_cache.restore_hash(saved); }
}

// ============================================================================
// Factory
// ============================================================================

fn truncate(s: &str, n: usize) -> &str { s.char_indices().nth(n).map(|(i,_)|&s[..i]).unwrap_or(s) }

pub fn create_cloud_client(config: &ModelConfig) -> Result<Box<dyn CloudClient>> {
    let api_key = config.api_key_env.as_deref()
        .and_then(|e| if e.is_empty() { None } else { std::env::var(e).ok() })
        .unwrap_or_default();
    if api_key.is_empty()
        && matches!(config.backend, ModelBackend::Anthropic | ModelBackend::OpenAI | ModelBackend::DeepSeek)
    {
        anyhow::bail!("API key not set for {} (env: {:?})", config.backend.name(), config.api_key_env);
    }
    let model = config.model.clone().unwrap_or_default()
        .split('[').next().unwrap_or_default().trim().to_string();
    if model.is_empty() { anyhow::bail!("model name not configured"); }
    let base_url = config.base_url.clone()
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .unwrap_or_else(|| match config.backend {
            ModelBackend::Anthropic => "https://api.anthropic.com".into(),
            ModelBackend::DeepSeek => "https://api.deepseek.com/v1".into(),
            ModelBackend::LmStudio => "http://localhost:1234/v1".into(),
            ModelBackend::Ollama => "http://localhost:11434/v1".into(),
            _ => "https://api.openai.com".into(),
        });
    match config.backend {
        ModelBackend::Anthropic => Ok(Box::new(crate::AnthropicClient::new(api_key, model, base_url))),
        _ => Ok(Box::new(OpenAIClient::new(api_key, model, base_url,
            matches!(config.backend, ModelBackend::LmStudio | ModelBackend::Ollama)))),
    }
}

pub async fn ensure_lm_studio_model(base_url: &str, model: &str, context_size: usize) -> Result<()> {
    let base = base_url.trim_end_matches('/');
    let models_url = if base.ends_with("/v1") { format!("{}/models", base) } else { format!("{}/v1/models", base) };
    let client = HttpClient::new();
    match client.get(&models_url).timeout(std::time::Duration::from_secs(5)).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                let ids: Vec<String> = body.get("data").and_then(|d| d.as_array())
                    .map(|arr| arr.iter()
                        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                        .collect())
                    .unwrap_or_default();
                if ids.iter().any(|id| id.split(':').next().unwrap_or(id) == model || id == model) {
                    tracing::debug!(%model, "LM Studio model already loaded");
                    return Ok(());
                }
            }
        }
        _ => {}
    }
    let load_url = format!("{}/api/v1/models/load", base.trim_end_matches("/v1"));
    let resp = client.post(&load_url)
        .json(&serde_json::json!({"model": model, "context_length": context_size}))
        .send().await?;
    if resp.status().is_success() { tracing::info!(%model, "LM Studio model loaded"); }
    else { tracing::debug!(%model, status=%resp.status(), "LM Studio load (non-fatal)"); }
    Ok(())
}

fn parse_inline_tool_calls(text: &str) -> (String, Vec<ToolCall>) {
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let open = "<｜｜DSML｜｜tool_calls>";
    let close = "";
    let mut cleaned = text.to_string();
    let mut search_from = 0;
    while let Some(start) = cleaned[search_from..].find(open) {
        let abs_start = search_from + start;
        let content_start = abs_start + open.len();
        if let Some(end) = cleaned[content_start..].find(close) {
            let content = &cleaned[content_start..content_start + end];
            let tc_json: serde_json::Value = serde_json::from_str(content).unwrap_or_default();
            let items = match tc_json {
                serde_json::Value::Array(arr) => arr,
                obj @ serde_json::Value::Object(_) => vec![obj],
                _ => vec![],
            };
            for item in items {
                let name = item["name"].as_str()
                    .or_else(|| item["function"]["name"].as_str())
                    .unwrap_or("unknown").to_string();
                let arguments = item.get("arguments")
                    .or_else(|| item["function"].get("arguments"))
                    .cloned().unwrap_or_default();
                tool_calls.push(ToolCall {
                    id: format!("dsml-{}", tool_calls.len()),
                    name,
                    arguments,
                });
            }
            let abs_end = content_start + end + close.len();
            cleaned.replace_range(abs_start..abs_end, "");
        } else {
            search_from = content_start;
        }
    }
    let cleaned = cleaned.trim().to_string();
    (cleaned, tool_calls)
}
