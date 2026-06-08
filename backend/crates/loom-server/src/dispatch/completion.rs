//! FIM (Fill-in-the-Middle) completion handler.
//! Calls DeepSeek's native /fim/completions endpoint directly.

use crate::AppState;
use loom_types::JsonRpcError;
use serde_json::Value;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "completion.fim" => Some(handle_completion_fim(state, p).await),
        _ => None,
    }
}

async fn handle_completion_fim(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let prefix = p
        .get("prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let suffix = p
        .get("suffix")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let model = p
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("deepseek-chat");
    // Try to resolve API key: params first, then key_store
    let api_key = p
        .get("api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let api_key = if api_key.is_empty() {
        state.key_store.read().await.get("DEEPSEEK_API_KEY").cloned().unwrap_or_default()
    } else {
        api_key.to_string()
    };
    let base_url = p
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://api.deepseek.com/beta");
    let max_tokens = p
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(64) as usize;

    if prefix.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "message": "prefix is required" }));
    }

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": model,
        "prompt": prefix,
        "suffix": suffix,
        "max_tokens": max_tokens,
        "temperature": 0.0,
        "stream": false,
    });

    let url = format!("{}/fim/completions", base_url.trim_end_matches('/'));

    match client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let json: Value = resp.json().await.unwrap_or_default();
            let text = json["choices"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string();
            Ok(serde_json::json!({ "ok": true, "completion": text }))
        }
        Ok(resp) => {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            tracing::warn!(%status, %err_text, "FIM completion failed");
            Ok(serde_json::json!({
                "ok": false,
                "message": format!("FIM API error {}: {}", status.as_u16(), err_text)
            }))
        }
        Err(e) => {
            tracing::warn!(error = %e, "FIM completion request failed");
            Ok(serde_json::json!({ "ok": false, "message": e.to_string() }))
        }
    }
}
