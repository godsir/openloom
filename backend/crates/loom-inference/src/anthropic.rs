//! Anthropic Messages API client.

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{CompletionRequest, CompletionResponse, ContentPart, Message, ModelBackend, ToolCall, ToolChoice};
use reqwest::Client as HttpClient;

use crate::engine::CloudClient;

pub struct AnthropicClient {
    api_key: String,
    model: String,
    base_url: String,
    http: HttpClient,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String, base_url: String) -> Self {
        Self { api_key, model, base_url, http: HttpClient::new() }
    }

    async fn complete_with_retry(&self, req: &CompletionRequest, retries: usize) -> Result<CompletionResponse> {
        let mut last_err = None;
        for attempt in 0..=retries {
            if attempt > 0 {
                let delay = 2u64.pow(attempt as u32) * 500;
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            match self.try_complete(req).await {
                Ok(resp) => return Ok(resp),
                Err(e) => { tracing::warn!(attempt, error = %e, "Anthropic API call failed"); last_err = Some(e); }
            }
        }
        Err(last_err.unwrap())
    }

    async fn try_complete(&self, req: &CompletionRequest) -> Result<CompletionResponse> {
        let messages = self.lower_messages(&req.effective_messages());
        let mut body = serde_json::json!({"model": self.model, "max_tokens": req.max_tokens, "messages": messages});
        if !req.tools.is_empty() {
            let tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
                serde_json::json!({"name": t.name, "description": t.description, "input_schema": t.input_schema})
            }).collect();
            body["tools"] = serde_json::json!(tools);
        }
        if let Some(ref tc) = req.tool_choice {
            match tc {
                ToolChoice::Auto => { body["tool_choice"] = serde_json::json!({"type": "auto"}); }
                ToolChoice::None => { body["tools"] = serde_json::json!([]); }
                ToolChoice::Required => { body["tool_choice"] = serde_json::json!({"type": "any"}); }
            }
        }
        if let Some(budget) = req.thinking_budget {
            body["thinking"] = serde_json::json!({"type": "enabled", "budget_tokens": budget});
        }

        let resp = self.http.post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key).header("anthropic-version", "2023-06-01")
            .json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {}: {}", status, text);
        }

        let body_text = resp.text().await?;
        let json: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            anyhow::anyhow!("Anthropic response parse error: {}, body: {}", e, truncate(&body_text, 500))
        })?;

        let (text, tool_calls, thinking) = self.parse_content(&json);
        let prompt_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize;
        let completion_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize;
        let cached_tokens = json["usage"]["cache_read_input_tokens"].as_u64().unwrap_or(0) as usize;

        Ok(CompletionResponse { text, tool_calls, prompt_tokens, completion_tokens, cached_tokens, latency_ms: 0, thinking })
    }

    fn lower_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
        messages.iter().map(|msg| {
            let role = msg.role.as_str();
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
            }).collect();
            serde_json::json!({"role": role, "content": content})
        }).collect()
    }

    fn parse_content(&self, json: &serde_json::Value) -> (String, Vec<ToolCall>, Option<String>) {
        let content = json["content"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        let text: String = content.iter()
            .filter_map(|b| if b["type"] == "text" { b["text"].as_str().map(|s| s.to_string()) } else { None })
            .collect::<Vec<_>>().join("\n");
        let tool_calls: Vec<ToolCall> = content.iter()
            .filter(|b| b["type"] == "tool_use")
            .filter_map(|b| Some(ToolCall { id: b["id"].as_str()?.to_string(), name: b["name"].as_str()?.to_string(), arguments: b["input"].clone() }))
            .collect();
        let thinking: Option<String> = {
            let texts: Vec<String> = content.iter()
                .filter_map(|b| if b["type"] == "thinking" { b["thinking"].as_str().map(|s| s.to_string()) } else { None })
                .collect();
            if texts.is_empty() { None } else { Some(texts.join("\n")) }
        };
        (text, tool_calls, thinking)
    }
}

fn truncate(s: &str, n: usize) -> &str { s.char_indices().nth(n).map(|(i,_)|&s[..i]).unwrap_or(s) }

#[async_trait]
impl CloudClient for AnthropicClient {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        self.complete_with_retry(&req, 3).await
    }

    async fn complete_stream(&self, req: CompletionRequest, tx: tokio::sync::mpsc::Sender<String>) -> Result<()> {
        let messages = self.lower_messages(&req.effective_messages());
        let mut body = serde_json::json!({"model": self.model, "max_tokens": req.max_tokens, "messages": messages, "stream": true});
        if let Some(budget) = req.thinking_budget {
            body["thinking"] = serde_json::json!({"type": "enabled", "budget_tokens": budget});
        }
        if !req.tools.is_empty() {
            let tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
                serde_json::json!({"name": t.name, "description": t.description, "input_schema": t.input_schema})
            }).collect();
            body["tools"] = serde_json::json!(tools);
        }

        let resp = self.http.post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key).header("anthropic-version", "2023-06-01")
            .json(&body).send().await?;

        if !resp.status().is_success() {
            anyhow::bail!("Anthropic API error {}: {}", resp.status(), resp.text().await.unwrap_or_default());
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
                        if let Some(text) = val["delta"]["text"].as_str()
                            && tx.send(text.to_string()).await.is_err() { return Ok(()); }
                        if let Some(usage) = val.get("message").and_then(|m| m.get("usage")) {
                            prompt_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
                            cached_tokens = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
                        }
                        if let Some(usage) = val.get("usage") {
                            completion_tokens = usage["output_tokens"].as_u64().unwrap_or(completion_tokens);
                        }
                    }
                }
            }
        }
        if prompt_tokens > 0 || completion_tokens > 0 {
            let _ = tx.send(format!("\x00USAGE:{}:{}:{}", prompt_tokens, completion_tokens, cached_tokens)).await;
        }
        Ok(())
    }

    fn provider(&self) -> ModelBackend { ModelBackend::Anthropic }
    fn model_name(&self) -> &str { &self.model }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_client_trait_object() {
        let client: Box<dyn CloudClient> = Box::new(AnthropicClient::new(
            "key".into(), "claude".into(), "https://api.anthropic.com".into(),
        ));
        assert_eq!(client.provider(), ModelBackend::Anthropic);
        assert_eq!(client.model_name(), "claude");
    }
}
