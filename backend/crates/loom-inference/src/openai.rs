//! OpenAI-compatible API client - OpenAI, DeepSeek, LM Studio, Ollama.

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{
    CompletionRequest, CompletionResponse, ContentPart, Message, ModelBackend, ModelConfig,
    StreamDelta, ToolCall, ToolChoice,
};
use reqwest::Client as HttpClient;

use crate::cache::PrefixCache;
use crate::engine::CloudClient;
use crate::engine::find_sse_boundary;

pub struct OpenAIClient {
    api_key: String,
    model: String,
    base_url: String,
    http: HttpClient,
    prefix_cache: PrefixCache,
}

impl OpenAIClient {
    pub fn new(api_key: String, model: String, base_url: String, _is_local: bool) -> Self {
        let http = HttpClient::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .unwrap_or_default();
        Self {
            api_key,
            model,
            base_url,
            http,
            prefix_cache: PrefixCache::new(2),
        }
    }

    async fn complete_with_retry(
        &self,
        req: &CompletionRequest,
        retries: usize,
    ) -> Result<CompletionResponse> {
        let mut last_err = None;
        for attempt in 0..=retries {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    2u64.pow(attempt as u32) * 500,
                ))
                .await;
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

    async fn try_complete(&self, req: &CompletionRequest) -> Result<CompletionResponse> {
        let eff = req.effective_messages();
        let (cache_hit, _) = self.prefix_cache.check(&eff);
        if cache_hit {
            tracing::info!("KV cache hit");
        } else {
            tracing::info!("KV cache miss");
        }
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
        body["temperature"] = req.temperature.into();
        if let Some(budget) = req.thinking_budget {
            let effort = if budget <= 2048 { "low" } else if budget <= 8192 { "medium" } else { "high" };
            body["reasoning_effort"] = serde_json::json!(effort);
        }

        // Debug: log request body length for troubleshooting (try_complete)
        let body_str = serde_json::to_string(&body).unwrap_or_default();
        tracing::debug!(
            body_len = body_str.len(),
            msg_count = messages.len(),
            tool_count = if req.tools.is_empty() { 0 } else { req.tools.len() },
            "OpenAI API request"
        );

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            // Log just the tools and last 2 messages for debugging
            let tools_json = serde_json::to_string_pretty(&req.tools).unwrap_or_default();
            let last_msgs: Vec<_> = messages.iter().rev().take(2).collect();
            let msgs_json = serde_json::to_string_pretty(&last_msgs).unwrap_or_default();
            tracing::error!(
                status = %resp.status(),
                body_len = serde_json::to_string(&body).unwrap_or_default().len(),
                tool_count = req.tools.len(),
                "API 400 — tools:\n{}\nlast_msgs:\n{}",
                tools_json,
                msgs_json
            );
            anyhow::bail!(
                "API error {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }

        let body_text = resp.text().await?;
        let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            anyhow::anyhow!(
                "API parse error: {}, body: {}",
                e,
                truncate(&body_text, 500)
            )
        })?;
        let choice = &json["choices"][0]["message"];

        // content can be a string or an array of parts (GPT-4o multimodal output)
        let (raw_text, images) = if choice["content"].is_array() {
            let parts = choice["content"].as_array().unwrap();
            let mut text = String::new();
            let mut imgs: Vec<(String, String)> = Vec::new();
            for part in parts {
                match part["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = part["text"].as_str() {
                            text.push_str(t);
                        }
                    }
                    Some("image_url") => {
                        if let Some(url) = part["image_url"]["url"].as_str() {
                            // data:image/png;base64,XXXX or https://...
                            if let Some(comma) = url.find(',') {
                                let media_type = url[5..comma]
                                    .trim_end_matches(";base64")
                                    .to_string();
                                let data = url[comma + 1..].to_string();
                                imgs.push((media_type, data));
                            }
                        }
                    }
                    _ => {}
                }
            }
            (text, imgs)
        } else {
            (choice["content"].as_str().unwrap_or("").to_string(), Vec::new())
        };

        let mut tool_calls: Vec<ToolCall> = choice["tool_calls"]
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
                            "parsing tool call arguments"
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
        if tool_calls.is_empty() {
            tool_calls = parse_inline_tool_calls(&raw_text).1;
        }
        Ok(CompletionResponse {
            text: if tool_calls.is_empty() {
                raw_text
            } else {
                String::new()
            },
            tool_calls,
            prompt_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize,
            completion_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize,
            cached_tokens: json["usage"]["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .unwrap_or(0) as usize,
            latency_ms: 0,
            thinking: choice["reasoning_content"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(String::from),
            images,
        })
    }

    fn lower_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
        messages.iter().map(|msg| {
            let role = msg.role.as_str();
            let mut obj = serde_json::json!({"role": role});
            if role == "tool" {
                if let Some(ContentPart::ToolResult { tool_call_id, name, result }) = msg.content.first() {
                    // Skip malformed tool messages with empty tool_call_id —
                    // these cause 400 errors with providers that validate the field
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
            let has_images = msg.content.iter().any(|p| matches!(p, ContentPart::Image { .. } | ContentPart::ImageRef { .. }));
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
                                "image_url": {"url": format!("data:{};base64,{}", media_type, data)},
                            }));
                        }
                        ContentPart::ImageRef { media_type, file_id } => {
                            tracing::warn!(
                                file_id = %file_id,
                                media_type = %media_type,
                                "ImageRef leaked to OpenAI inference layer — image omitted. \
                                 The agent loop should strip ImageRef from history before inference."
                            );
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
            let texts: Vec<&str> = msg.content.iter()
                .filter_map(|p| match p { ContentPart::Text { text } => Some(text.as_str()), _ => None })
                .collect();
            let thinking_texts: Vec<String> = msg.content.iter().filter_map(|p| match p {
                ContentPart::Thinking { text } => Some(format!("[reasoning]\n{}", text)),
                _ => None,
            }).collect();
            let tc_vals: Vec<serde_json::Value> = msg.content.iter().filter_map(|p| match p {
                ContentPart::ToolCall { id, name, arguments } => Some(serde_json::json!({
                    "id": id, "type": "function",
                    "function": {"name": name, "arguments": serde_json::to_string(arguments).unwrap_or_default()},
                })),
                _ => None,
            }).collect();
            let all_texts: Vec<&str> = texts.iter().map(|s| *s)
                .chain(thinking_texts.iter().map(|s| s.as_str()))
                .collect();
            if all_texts.is_empty() && tc_vals.is_empty() {
                obj["content"] = serde_json::json!("");
            } else if !all_texts.is_empty() {
                obj["content"] = serde_json::json!(all_texts.join("\n"));
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

    async fn complete_stream(
        &self,
        req: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        let eff = req.effective_messages();
        let (cache_hit, _) = self.prefix_cache.check(&eff);
        if cache_hit {
            tracing::info!("KV cache hit (stream)");
        } else {
            tracing::info!("KV cache miss (stream)");
        }
        let messages = self.lower_messages(&eff);
        let mut body = serde_json::json!({
            "model": self.model, "max_tokens": req.max_tokens, "messages": messages,
            "stream": true, "stream_options": {"include_usage": true},
        });
        body["temperature"] = req.temperature.into();
        if let Some(budget) = req.thinking_budget {
            let effort = if budget <= 2048 { "low" } else if budget <= 8192 { "medium" } else { "high" };
            body["reasoning_effort"] = serde_json::json!(effort);
        }
        if !req.tools.is_empty() {
            body["tools"] = serde_json::json!(req.tools.iter().map(|t| serde_json::json!({
                "type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema},
            })).collect::<Vec<_>>());
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
            // Log just the tools and last 2 messages for debugging
            let tools_json = serde_json::to_string_pretty(&req.tools).unwrap_or_default();
            let last_msgs: Vec<_> = messages.iter().rev().take(2).collect();
            let msgs_json = serde_json::to_string_pretty(&last_msgs).unwrap_or_default();
            tracing::error!(
                status = %resp.status(),
                body_len = serde_json::to_string(&body).unwrap_or_default().len(),
                tool_count = req.tools.len(),
                "API 400 — tools:\n{}\nlast_msgs:\n{}",
                tools_json,
                msgs_json
            );
            anyhow::bail!(
                "API error {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
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
                                && tx.send(format!("\x02REASONING\x02{}", r)).await.is_err()
                            {
                                return Ok(());
                            }
                            if let Some(t) = d["content"].as_str()
                                && tx.send(t.to_string()).await.is_err()
                            {
                                return Ok(());
                            }
                            if let Some(u) = val.get("usage").filter(|u| u.is_object()) {
                                let _ = tx
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

    async fn complete_stream_structured(
        &self,
        req: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamDelta>,
    ) -> Result<()> {
        let eff = req.effective_messages();
        let (cache_hit, _) = self.prefix_cache.check(&eff);
        if cache_hit {
            tracing::info!("KV cache hit (structured stream)");
        } else {
            tracing::info!("KV cache miss (structured stream)");
        }
        let messages = self.lower_messages(&eff);
        let mut body = serde_json::json!({
            "model": self.model, "max_tokens": req.max_tokens, "messages": messages,
            "stream": true, "stream_options": {"include_usage": true},
        });
        body["temperature"] = req.temperature.into();
        if let Some(budget) = req.thinking_budget {
            let effort = if budget <= 2048 { "low" } else if budget <= 8192 { "medium" } else { "high" };
            body["reasoning_effort"] = serde_json::json!(effort);
        }
        if !req.tools.is_empty() {
            body["tools"] = serde_json::json!(req.tools.iter().map(|t| serde_json::json!({
                "type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema},
            })).collect::<Vec<_>>());
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
            // Log just the tools and last 2 messages for debugging
            let tools_json = serde_json::to_string_pretty(&req.tools).unwrap_or_default();
            let last_msgs: Vec<_> = messages.iter().rev().take(2).collect();
            let msgs_json = serde_json::to_string_pretty(&last_msgs).unwrap_or_default();
            tracing::error!(
                status = %resp.status(),
                body_len = serde_json::to_string(&body).unwrap_or_default().len(),
                tool_count = req.tools.len(),
                "API 400 — tools:\n{}\nlast_msgs:\n{}",
                tools_json,
                msgs_json
            );
            anyhow::bail!(
                "API error {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
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
        ModelBackend::OpenAI
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

// ============================================================================
// Factory
// ============================================================================

fn truncate(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map(|(i, _)| &s[..i]).unwrap_or(s)
}

pub fn create_cloud_client(config: &ModelConfig, api_key: &str) -> Result<Box<dyn CloudClient>> {
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
    let base_url = config
        .base_url
        .clone()
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .unwrap_or_else(|| match config.backend {
            ModelBackend::Anthropic => "https://api.anthropic.com".into(),
            ModelBackend::DeepSeek => "https://api.deepseek.com/v1".into(),
            ModelBackend::LmStudio => "http://localhost:1234/v1".into(),
            ModelBackend::Ollama => "http://localhost:11434/v1".into(),
            _ => "https://api.openai.com".into(),
        });
    match config.backend {
        ModelBackend::Anthropic => Ok(Box::new(crate::AnthropicClient::new(
            api_key.to_string(), model, base_url,
        ))),
        _ => Ok(Box::new(OpenAIClient::new(
            api_key.to_string(),
            model,
            base_url,
            matches!(
                config.backend,
                ModelBackend::LmStudio | ModelBackend::Ollama
            ),
        ))),
    }
}

pub async fn ensure_lm_studio_model(
    base_url: &str,
    model: &str,
    context_size: usize,
) -> Result<()> {
    let base = base_url.trim_end_matches('/');
    let models_url = if base.ends_with("/v1") {
        format!("{}/models", base)
    } else {
        format!("{}/v1/models", base)
    };
    let client = HttpClient::new();
    match client
        .get(&models_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                let ids: Vec<String> = body
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if ids
                    .iter()
                    .any(|id| id.split(':').next().unwrap_or(id) == model || id == model)
                {
                    tracing::debug!(%model, "LM Studio model already loaded");
                    return Ok(());
                }
            }
        }
        _ => {}
    }
    let load_url = format!("{}/api/v1/models/load", base.trim_end_matches("/v1"));
    let resp = client
        .post(&load_url)
        .json(&serde_json::json!({"model": model, "context_length": context_size}))
        .send()
        .await?;
    if resp.status().is_success() {
        tracing::info!(%model, "LM Studio model loaded");
    } else {
        tracing::debug!(%model, status=%resp.status(), "LM Studio load (non-fatal)");
    }
    Ok(())
}

/// Parse tool call arguments from the LLM response.
/// Handles both the standard OpenAI string format ("{\"path\":\"...\"}")
/// and the object format used by some proxies/gateways ({"path":"..."}).
pub(crate) fn parse_tool_arguments(args: &serde_json::Value) -> serde_json::Value {
    // Standard OpenAI format: arguments is a JSON-encoded string
    if let Some(s) = args.as_str() {
        match serde_json::from_str(s) {
            Ok(v) => return v,
            Err(e) => {
                tracing::warn!(raw = %s, error = %e, "failed to parse tool arguments string");
                return serde_json::Value::Object(serde_json::Map::new());
            }
        }
    }
    // Some gateways/proxies return arguments as a JSON object directly
    if args.is_object() {
        return args.clone();
    }
    // Log unexpected format for debugging
    tracing::warn!(
        args_type = %format!("{:?}", args),
        "unexpected tool arguments format — received neither string nor object"
    );
    serde_json::Value::Object(serde_json::Map::new())
}

pub fn parse_inline_tool_calls(text: &str) -> (String, Vec<ToolCall>) {
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut cleaned = text.to_string();

    // Strategy 1: Parse bare JSON tool call blocks from the text.
    // Local models sometimes emit tool calls as raw JSON text instead of
    // structured function calls, e.g.:
    //   {"tool": "file_write", "arguments": {"path": "/foo"}}
    //   {"name": "file_write", "arguments": {"path": "/foo"}}
    //   {"tool": "request_tools", "arguments": {"tools": ["file_system"]}}
    let mut search_from = 0;
    while let Some(brace_start) = cleaned[search_from..].find('{') {
        let abs_start = search_from + brace_start;
        // Find matching closing brace by counting depth
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape_next = false;
        let mut end_pos = None;
        for (i, ch) in cleaned[abs_start..].char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }
            match ch {
                '\\' if in_string => { escape_next = true; }
                '"' => { in_string = !in_string; }
                '{' if !in_string => { depth += 1; }
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = Some(abs_start + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let abs_end = match end_pos {
            Some(e) => e,
            None => { search_from = abs_start + 1; continue; }
        };
        let json_str = &cleaned[abs_start..abs_end];
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
            // Accept objects that look like tool calls
            let name = val["tool"].as_str()
                .or_else(|| val["name"].as_str())
                .or_else(|| val["function"]["name"].as_str());
            if let Some(name) = name {
                if !name.is_empty() && name.len() < 64
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    let arguments = val.get("arguments")
                        .or_else(|| val["function"].get("arguments"))
                        .cloned()
                        .unwrap_or(serde_json::json!({}));
                    tool_calls.push(ToolCall {
                        id: format!("inline-{}", tool_calls.len()),
                        name: name.to_string(),
                        arguments,
                    });
                    cleaned.replace_range(abs_start..abs_end, "");
                    continue;
                }
            }
        }
        search_from = abs_end;
    }

    // Strategy 2: Parse DeepSeek XML tool call format
    let (cleaned, xml_calls) = parse_xml_tool_calls(&cleaned);
    tool_calls.extend(xml_calls);

    let cleaned = cleaned.trim().to_string();
    (cleaned, tool_calls)
}

/// Parse DeepSeek XML tool call formats that appear when native tool calling is
/// unavailable.
///
/// Format A (special Unicode markers):
/// ```text
/// <｜tool▁calls▁begin｜><｜tool▁call▁begin｜>function<｜tool▁sep｜>shell
/// ```json
/// {"command": "ls"}
/// ```<｜tool▁call▁end｜><｜tool▁calls▁end｜>
/// ```
///
/// Format B (standard XML):
/// ```xml
/// <tool_calls>
/// <invoke name="shell">
/// <parameter name="command">echo hi</parameter>
/// </invoke>
/// </tool_calls>
/// ```
fn parse_xml_tool_calls(text: &str) -> (String, Vec<ToolCall>) {
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut cleaned = text.to_string();

    // ── Format A: <｜tool▁calls▁begin｜>…<｜tool▁calls▁end｜> ──
    if let Some(start) = cleaned.find("<｜tool▁calls▁begin｜>") {
        if let Some(end) = cleaned.find("<｜tool▁calls▁end｜>") {
            let block = &cleaned[start..end + "<｜tool▁calls▁end｜>".len()];
            let mut search_from = 0;
            while let Some(call_start) = block[search_from..].find("<｜tool▁call▁begin｜>function<｜tool▁sep｜>") {
                let abs_start = search_from + call_start;
                if let Some(call_end) = block[abs_start..].find("<｜tool▁call▁end｜>") {
                    let call_block = &block[abs_start..abs_start + call_end + "<｜tool▁call▁end｜>".len()];
                    if let Some(name_start) = call_block.find("<｜tool▁sep｜>") {
                        let after_sep = &call_block[name_start + "<｜tool▁sep｜>".len()..];
                        let name_end = after_sep.find('\n').unwrap_or(after_sep.len());
                        let name = after_sep[..name_end].trim().to_string();
                        let args = if let Some(json_start) = call_block.find("```json") {
                            let after_json = &call_block[json_start + 7..];
                            if let Some(json_end) = after_json.find("```") {
                                let json_str = after_json[..json_end].trim();
                                serde_json::from_str(json_str).unwrap_or(serde_json::json!({}))
                            } else {
                                serde_json::json!({})
                            }
                        } else {
                            serde_json::json!({})
                        };
                        if !name.is_empty() {
                            tool_calls.push(ToolCall {
                                id: format!("xml-a-{}", tool_calls.len()),
                                name,
                                arguments: args,
                            });
                        }
                    }
                    search_from = abs_start + call_end + "<｜tool▁call▁end｜>".len();
                } else {
                    break;
                }
            }
            cleaned = format!("{}{}", &cleaned[..start], &cleaned[end + "<｜tool▁calls▁end｜>".len()..]);
        }
    }

    // ── Format B: <tool_calls><invoke name="X"><parameter name="Y">…</parameter></invoke></tool_calls> ──
    // Match any open-tag containing "tool_calls" (handles Unicode prefix variants).
    let re_tc_start = match regex_find(&cleaned, "<[^>]*tool_calls[^>]*>") {
        Some((s, _)) => s,
        None => return (cleaned, tool_calls),
    };
    let tc_start = re_tc_start;
    // Find the matching close tag: any tag containing "tool_calls" preceded by "/"
    // This handles prefix mismatches like <hz_tool_calls> … </tool_calls>
    let tc_end = {
        let after = &cleaned[tc_start + 1..];
        // Find the next closing tag that contains "tool_calls"
        let mut search = 0;
        let mut found = None;
        while let Some(slash) = after[search..].find("</") {
            let abs_slash = search + slash;
            if let Some(gt) = after[abs_slash..].find('>') {
                let tag = &after[abs_slash..abs_slash + gt + 1];
                if tag.contains("tool_calls") {
                    found = Some(tc_start + 1 + abs_slash + gt + 1);
                    break;
                }
                search = abs_slash + 1;
            } else {
                break;
            }
        }
        match found {
            Some(e) => e,
            None => return (cleaned, tool_calls),
        }
    };
    let block = &cleaned[tc_start..tc_end].to_string();

    // Find all <invoke ...> … </invoke> blocks
    let mut invoke_search = 0;
    while let Some(inv_open) = block[invoke_search..].find("<invoke") {
        let inv_start = invoke_search + inv_open;
        // Find the end of the opening tag
        let Some(tag_body_end) = block[inv_start..].find('>') else { break };
        let open_tag = &block[inv_start..inv_start + tag_body_end + 1];

        // Extract tool name from name="X"
        let name =
            open_tag
                .split("name=")
                .nth(1)
                .and_then(|s| {
                    let s = s.trim_start_matches('"').trim_start_matches('\'');
                    let end = s.find(|c| c == '"' || c == '\'')?;
                    Some(s[..end].to_string())
                })
                .unwrap_or_default();

        // Find closing </invoke> — some models use a prefixed tag, so
        // look for any tag ending with "invoke>"
        let inv_end_tag = if let Some(pos) = block[inv_start + tag_body_end + 1..].find("</invoke") {
            // skip to after the >
            if let Some(gt) = block[inv_start + tag_body_end + 1 + pos..].find('>') {
                inv_start + tag_body_end + 1 + pos + gt + 1
            } else {
                invoke_search = inv_start + 1;
                continue;
            }
        } else {
            invoke_search = inv_start + 1;
            continue;
        };
        let invoke_body = &block[inv_start + tag_body_end + 1..inv_end_tag];

        // Extract parameters
        let mut args = serde_json::Map::new();
        let mut param_search = 0;
        while let Some(param_open) = invoke_body[param_search..].find("<parameter") {
            let param_start = param_search + param_open;
            let Some(param_tag_end) = invoke_body[param_start..].find('>') else { break };
            let param_tag = &invoke_body[param_start..param_start + param_tag_end + 1];

            let p_name = param_tag
                .split("name=")
                .nth(1)
                .and_then(|s| {
                    let s = s.trim_start_matches('"').trim_start_matches('\'');
                    let end = s.find(|c| c == '"' || c == '\'')?;
                    Some(s[..end].to_string())
                })
                .unwrap_or_default();

            // Value is the text between <parameter...> and </parameter> (or similar closing tag)
            let param_close = if let Some(pos) = invoke_body[param_start + param_tag_end + 1..].find("</parameter") {
                param_start + param_tag_end + 1 + pos
            } else {
                param_search = param_start + 1;
                continue;
            };
            // Find actual close > for the closing tag
            let value_end = if let Some(close_gt) = invoke_body[param_close..].find('>') {
                param_close + close_gt + 1
            } else {
                param_search = param_start + 1;
                continue;
            };
            if !p_name.is_empty() {
                let value = &invoke_body[param_start + param_tag_end + 1..param_close];
                args.insert(p_name, serde_json::Value::String(value.trim().to_string()));
            }
            param_search = value_end;
        }

        if !name.is_empty() {
            tool_calls.push(ToolCall {
                id: format!("xml-b-{}", tool_calls.len()),
                name,
                arguments: serde_json::Value::Object(args),
            });
        }
        invoke_search = inv_end_tag;
    }

    // Remove the entire tool_calls block from text
    cleaned = format!("{}{}", &cleaned[..tc_start], &cleaned[tc_end..]);

    (cleaned, tool_calls)
}

/// Find a regex-like pattern in text. Returns (start_index, end_index).
/// Simple helper — scans for `<` then looks ahead for the pattern.
fn regex_find(text: &str, _pattern: &str) -> Option<(usize, usize)> {
    // pattern like "<[^>]*tool_calls[^>]*>"
    // We look for '<', then skip non-'>' chars until we find "tool_calls", then skip to '>'
    let mut i = 0;
    while let Some(lt) = text[i..].find('<') {
        let abs = i + lt;
        if let Some(gt) = text[abs..].find('>') {
            let tag = &text[abs..abs + gt + 1];
            if tag.contains("tool_calls") {
                return Some((abs, abs + gt + 1));
            }
            i = abs + 1;
        } else {
            break;
        }
    }
    None
}
