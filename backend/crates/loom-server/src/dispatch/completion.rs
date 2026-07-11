//! FIM (Fill-in-the-Middle) completion handler.
//! Uses the FIM model configured via config.get_fim / config.set_fim.
//! Model config (supplier settings) provides the actual base_url, model ID, and api_key.

use crate::AppState;
use loom_types::JsonRpcError;
use loom_types::config::model_config::ModelBackend;
use serde_json::{Value, json};

/// Fallback FIM endpoint — only used when nothing is configured.
const DEFAULT_FIM_BASE_URL: &str = "https://api.deepseek.com/beta";

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "completion.fim" => Some(handle_completion_fim(state, p).await),
        "completion.chat" => Some(handle_completion_chat(state, p).await),
        _ => None,
    }
}

/// Load saved FIM config from ~/.loom/fim.json
fn load_fim_config() -> (Option<String>, Option<String>, Option<String>) {
    let home = dirs::home_dir().unwrap_or_default().join(".loom");
    let config_file = home.join("fim.json");
    let config: Value = std::fs::read_to_string(&config_file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(json!({}));

    let model = config
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let base_url = config
        .get("base_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let api_key_env = config
        .get("api_key_env")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    (model, base_url, api_key_env)
}

/// Resolve model ID, base_url, api_key_env, and backend from the user's model config (supplier settings).
/// Returns (model_id, base_url, api_key_env, backend).
async fn resolve_fim_model(
    state: &AppState,
    model_name: &str,
) -> (String, String, String, ModelBackend) {
    let mut model = String::new();
    let mut base_url = String::new();
    let mut api_key_env = String::new();
    let mut backend = ModelBackend::default();

    if let Ok(config) = state.orchestrator.model_config_get(model_name).await {
        model = config.model.unwrap_or_default();
        base_url = config.base_url.unwrap_or_default();
        api_key_env = config.api_key_env.unwrap_or_default();
        backend = config.backend;
    }

    (model, base_url, api_key_env, backend)
}

/// Derive a base URL from the backend when the model config doesn't specify one.
/// For DeepSeek, uses /beta (the FIM endpoint path). For other backends, uses the
/// standard /v1 API base. Mirrors orchestrator::try_build_cloud_client otherwise.
fn backend_default_base_url(backend: &ModelBackend) -> &'static str {
    match backend {
        ModelBackend::DeepSeek => "https://api.deepseek.com/beta", // FIM is at /beta, not /v1
        ModelBackend::LmStudio => "http://localhost:1234/v1",
        ModelBackend::Ollama => "http://localhost:11434/v1",
        ModelBackend::Anthropic => "https://api.anthropic.com",
        ModelBackend::OpenAI => "https://api.openai.com",
        ModelBackend::Custom => "http://localhost:8080/v1",
    }
}

/// Derive a default API key environment variable name from the backend.
fn backend_default_key_env(backend: &ModelBackend) -> &'static str {
    match backend {
        ModelBackend::DeepSeek => "DEEPSEEK_API_KEY",
        ModelBackend::OpenAI => "OPENAI_API_KEY",
        ModelBackend::Anthropic => "ANTHROPIC_API_KEY",
        _ => "OPENLOOM_API_KEY",
    }
}

async fn handle_completion_fim(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let prefix = p.get("prefix").and_then(|v| v.as_str()).unwrap_or("");
    let suffix = p.get("suffix").and_then(|v| v.as_str()).unwrap_or("");
    let max_tokens = p.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(64) as usize;

    if prefix.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "message": "prefix is required" }));
    }

    // 1. Resolve model name: saved FIM config (fresh from disk) → request param → default
    let (saved_model, saved_fim_base_url, saved_fim_key_env) = load_fim_config();

    let model_name = saved_model
        .as_deref()
        .or(p.get("model").and_then(|v| v.as_str()))
        .unwrap_or("deepseek-chat");

    // 2. Resolve from user's supplier settings (model config)
    let (model, config_base_url, config_key_env, config_backend) =
        resolve_fim_model(state, model_name).await;

    // 3. base_url priority:
    //    a) fim.json override (advanced users)
    //    b) Model config base_url (from supplier settings — what normal users configure)
    //    c) Backend-based default (matching orchestrator::try_build_cloud_client)
    //    d) Hardcoded fallback
    let base_url = saved_fim_base_url
        .filter(|s| !s.is_empty())
        .or_else(|| {
            if !config_base_url.is_empty() {
                Some(config_base_url)
            } else {
                None
            }
        })
        .or_else(|| Some(backend_default_base_url(&config_backend).to_string()))
        .unwrap_or_else(|| DEFAULT_FIM_BASE_URL.to_string());

    // 4. API key priority:
    //    a) Request param (frontend override)
    //    b) fim.json env name → key_store lookup
    //    c) Model config env name → key_store lookup
    //    d) Backend-based default env name → key_store lookup
    let api_key = p
        .get("api_key")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    let api_key: String = match api_key {
        Some(k) => k.to_string(),
        None => {
            let env_name = saved_fim_key_env
                .as_deref()
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    if !config_key_env.is_empty() {
                        Some(&config_key_env[..])
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| backend_default_key_env(&config_backend));
            state
                .key_store
                .read()
                .await
                .get(env_name)
                .cloned()
                .unwrap_or_default()
        }
    };

    // 5. Model ID: user config value → model_name as fallback
    let model = if model.is_empty() {
        model_name.to_string()
    } else {
        model
    };

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": model,
        "prompt": prefix,
        "suffix": suffix,
        "max_tokens": max_tokens,
        "temperature": 0.0,
        "stream": false,
    });

    // Build FIM URL
    // DeepSeek FIM: POST https://api.deepseek.com/beta/completions
    // OpenAI-compatible: POST {base_url}/completions (with "suffix" field for FIM)
    let url = if base_url.ends_with("/completions") {
        base_url.to_string()
    } else {
        format!("{}/completions", base_url.trim_end_matches('/'))
    };

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
            tracing::warn!(%status, %err_text, %url, %model, "FIM completion failed");
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

/// Simple chat completion — uses the orchestrator's cloud client (active model).
/// Speaks OpenAI-compatible HTTP to the configured provider.
async fn handle_completion_chat(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let messages_raw = p
        .get("messages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let max_tokens =
        (p.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(200) as usize).clamp(1, 4096);
    let temperature = p.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;

    if messages_raw.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "message": "messages is required" }));
    }

    // Build chat messages
    let messages: Vec<serde_json::Value> = messages_raw
        .iter()
        .filter_map(|m| {
            let role = m.get("role")?.as_str()?;
            let content = m.get("content")?.as_str()?;
            Some(serde_json::json!({ "role": role, "content": content }))
        })
        .collect();

    if messages.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "message": "no valid messages" }));
    }

    // Use the orchestrator's active model to resolve config
    let active_name = state.orchestrator.active_model_name().await;

    let (model_id, base_url, api_key) = if let Some(ref name) = active_name {
        let config = match state.orchestrator.model_config_get(name).await {
            Ok(c) => c,
            Err(_) => {
                return Ok(
                    serde_json::json!({ "ok": false, "message": format!("model config '{}' not found", name) }),
                );
            }
        };
        let model = config.model.clone().unwrap_or_else(|| config.name.clone());
        let url = config
            .base_url
            .clone()
            .unwrap_or_else(|| match config.backend {
                ModelBackend::LmStudio => "http://localhost:1234/v1".into(),
                ModelBackend::Ollama => "http://localhost:11434/v1".into(),
                ModelBackend::OpenAI => "https://api.openai.com/v1".into(),
                ModelBackend::DeepSeek => "https://api.deepseek.com/v1".into(),
                ModelBackend::Anthropic => "https://api.anthropic.com".into(),
                ModelBackend::Custom => "http://localhost:8080/v1".into(),
            });
        let key = state
            .orchestrator
            .resolve_api_key(config.api_key_env.as_deref(), &config.backend)
            .await
            .unwrap_or_default();
        // Anthropic uses /v1/messages, not /chat/completions — unsupported via this path
        if matches!(config.backend, ModelBackend::Anthropic) {
            return Ok(
                serde_json::json!({ "ok": false, "message": "Anthropic backend is not supported for completion.chat; use an OpenAI-compatible provider" }),
            );
        }
        (model, url, key)
    } else {
        return Ok(serde_json::json!({ "ok": false, "message": "no active model configured" }));
    };

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": model_id,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "stream": false,
    });

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    tracing::info!(%url, %model_id, "completion.chat sending request");

    match client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let json: Value = resp.json().await.unwrap_or_default();
            tracing::info!(body = %json.to_string().chars().take(500).collect::<String>(), "completion.chat raw response");
            let content = json["choices"][0]["message"]["content"]
                .as_str()
                .filter(|s| !s.is_empty());
            let reasoning = json["choices"][0]["message"]["reasoning_content"]
                .as_str()
                .filter(|s| !s.is_empty());
            let text = content.or(reasoning).unwrap_or("").to_string();
            tracing::info!(len = text.len(), "completion.chat extracted text");
            Ok(serde_json::json!({ "ok": true, "content": text }))
        }
        Ok(resp) => {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            tracing::warn!(%status, %err_text, %url, %model_id, "chat completion failed");
            Ok(serde_json::json!({
                "ok": false,
                "message": format!("API error {}: {}", status.as_u16(), err_text)
            }))
        }
        Err(e) => {
            tracing::warn!(error = %e, "chat completion request failed");
            Ok(serde_json::json!({ "ok": false, "message": e.to_string() }))
        }
    }
}
