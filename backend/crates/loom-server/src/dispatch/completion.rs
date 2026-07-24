//! FIM (Fill-in-the-Middle) completion handler.
//! Uses the FIM model configured via config.get_fim / config.set_fim.
//! Model config (supplier settings) provides the actual base_url, model ID, and api_key.

use crate::AppState;
use loom_inference::engine::build_http_client;
use loom_types::JsonRpcError;
use loom_types::config::model_config::ModelBackend;
use serde_json::Value;

/// Fallback FIM endpoint — only used when nothing is configured.
const DEFAULT_FIM_BASE_URL: &str = "https://api.deepseek.com/beta";

/// Conservative prompt-char budget derived from the model's context window.
/// CJK text tokenizes at ~1 token/char on cl100k; reserve the output tokens
/// plus a fixed safety margin, then keep 25% headroom for token-denser text.
/// Returns None when the window is unknown (0 = unset) so requests to
/// providers without a configured context size stay unclamped.
fn prompt_char_budget(context_size: usize, max_tokens: usize) -> Option<usize> {
    if context_size == 0 {
        return None;
    }
    let usable = context_size.saturating_sub(max_tokens + 256);
    Some(usable * 3 / 4)
}

/// Trim `content` to `budget` chars. Marker-aware: inline-edit prompts carry
/// <<<PREFIX / <<<EDIT_SCOPE / <<<SUFFIX sections — the EDIT_SCOPE (the text
/// being rewritten) is the payload and is NEVER trimmed (a partial rewrite
/// applied back to the full selection would silently lose user text). PREFIX
/// is kept tail-first (nearest context matters), the suffix/instruction region
/// is middle-elided. Returns None when the EDIT_SCOPE block alone exceeds the
/// budget — the caller must surface a clear error instead of sending a
/// crippled prompt. Operates on char boundaries (no mid-UTF8 cut).
fn clamp_content_for_budget(content: &str, budget: usize) -> Option<String> {
    if content.chars().count() <= budget {
        return Some(content.to_string());
    }
    const ELISION: &str = "\n...[已截断]...\n";
    let el = ELISION.chars().count();

    if let (Some(scope_start), Some(suffix_marker)) =
        (content.find("<<<EDIT_SCOPE"), content.find("<<<SUFFIX"))
    {
        let head_region = &content[..scope_start]; // <<<PREFIX + prefix text
        let scope_region = &content[scope_start..suffix_marker]; // EDIT_SCOPE block
        let tail_region = &content[suffix_marker..]; // <<<SUFFIX + suffix + instruction
        let (hl, sl, tl) = (
            head_region.chars().count(),
            scope_region.chars().count(),
            tail_region.chars().count(),
        );
        if sl + 2 * el > budget {
            return None;
        }
        let remaining = budget - sl - 2 * el;
        let keep_head = remaining / 3;
        let keep_tail = remaining - keep_head;
        let head_part: String = if hl <= keep_head {
            head_region.to_string()
        } else {
            head_region.chars().skip(hl - keep_head).collect()
        };
        let tail_part: String = if tl <= keep_tail {
            tail_region.to_string()
        } else {
            let h = keep_tail / 2;
            let t = keep_tail - h;
            let h: String = tail_region.chars().take(h).collect();
            let t: String = tail_region.chars().skip(tl - t).collect();
            format!("{h}{ELISION}{t}")
        };
        return Some(format!("{ELISION}{head_part}{ELISION}{scope_region}{tail_part}"));
    }

    if budget <= el + 32 {
        // Budget too small for meaningful elision — hard tail-keep.
        return Some(
            content
                .chars()
                .skip(content.chars().count().saturating_sub(budget))
                .collect(),
        );
    }
    let head_len = (budget - el) * 2 / 3;
    let tail_len = budget - el - head_len;
    let total = content.chars().count();
    let head: String = content.chars().take(head_len).collect();
    let tail: String = content.chars().skip(total - tail_len).collect();
    Some(format!("{head}{ELISION}{tail}"))
}

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "completion.fim" => Some(handle_completion_fim(state, p).await),
        "completion.fim_probe" => {
            let mut probe = p.clone();
            if let Some(obj) = probe.as_object_mut() {
                obj.insert("prefix".into(), Value::String("The quick ".into()));
                obj.insert("suffix".into(), Value::String(" fox".into()));
                obj.insert("max_tokens".into(), Value::from(8));
            }
            Some(handle_completion_fim(state, &probe).await)
        }
        "completion.chat" => Some(handle_completion_chat(state, p).await),
        _ => None,
    }
}

/// Load saved FIM config from the unified config store (~/.loom/config.json).
async fn load_fim_config(state: &AppState) -> (Option<String>, Option<String>, Option<String>) {
    let fim = state.orchestrator.config_store().fim().await;
    (fim.model, fim.base_url, fim.api_key_env)
}

/// Resolve model ID, base_url, api_key_env, backend, and context_size from the user's model config (supplier settings).
/// Returns (model_id, base_url, api_key_env, backend, context_size).
async fn resolve_fim_model(
    state: &AppState,
    model_name: &str,
) -> (String, String, String, ModelBackend, usize) {
    let mut model = String::new();
    let mut base_url = String::new();
    let mut api_key_env = String::new();
    let mut backend = ModelBackend::default();
    let mut context_size = 0usize;

    if let Ok(config) = state.orchestrator.model_config_get(model_name).await {
        model = config.model.unwrap_or_default();
        base_url = config.base_url.unwrap_or_default();
        api_key_env = config.api_key_env.unwrap_or_default();
        backend = config.backend;
        context_size = config.context_size;
    }

    (model, base_url, api_key_env, backend, context_size)
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
        ModelBackend::OpenAI => "https://api.openai.com/v1",
        ModelBackend::Custom => "http://localhost:8080/v1",
    }
}

async fn handle_completion_fim(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let prefix_raw = p.get("prefix").and_then(|v| v.as_str()).unwrap_or("");
    let suffix_raw = p.get("suffix").and_then(|v| v.as_str()).unwrap_or("");
    let max_tokens = (p.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(64) as usize)
        .clamp(1, 256);

    if prefix_raw.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "message": "prefix is required" }));
    }

    // 1. Resolve model name: saved FIM config (fresh from disk) → request param → default
    let (saved_model, saved_fim_base_url, saved_fim_key_env) = load_fim_config(state).await;

    let Some(model_name) = p.get("model").and_then(|v| v.as_str())
        .or(saved_model.as_deref())
        .filter(|name| !name.is_empty())
    else {
        return Ok(serde_json::json!({
            "ok": false,
            "message": "No FIM model configured. Select a compatible model in Settings."
        }));
    };

    // 2. Resolve from user's supplier settings (model config)
    let (model, config_base_url, config_key_env, config_backend, fim_context_size) =
        resolve_fim_model(state, model_name).await;

    // Context-aware window: local small-window models can't take the full
    // 32K/16K char windows. Prefix keeps the TAIL (nearest context matters
    // most for FIM), suffix keeps the HEAD.
    let (prefix_cap, suffix_cap) = match prompt_char_budget(fim_context_size, max_tokens) {
        Some(budget) => ((budget * 2 / 3).max(256), (budget / 3).max(128)),
        None => (32_000, 16_000),
    };
    let prefix = {
        let mut chars: Vec<char> = prefix_raw.chars().rev().take(prefix_cap).collect();
        chars.reverse();
        chars.into_iter().collect::<String>()
    };
    let suffix = suffix_raw.chars().take(suffix_cap).collect::<String>();

    // 3. base_url priority:
    //    a) FIM config override (advanced users)
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
    //    b) FIM config key reference
    //    c) Model config key reference
    // The shared resolver also handles literal keys, the process environment,
    // persisted credentials and backend-specific defaults.
    let request_api_key = p
        .get("api_key")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    let api_key = match request_api_key {
        Some(k) => k.to_string(),
        None => {
            let key_reference = saved_fim_key_env
                .as_deref()
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    if !config_key_env.is_empty() {
                        Some(&config_key_env[..])
                    } else {
                        None
                    }
                });
            state
                .orchestrator
                .resolve_api_key(key_reference, &config_backend)
                .await
                .unwrap_or_default()
        }
    };
    if api_key.is_empty() {
        return Ok(serde_json::json!({
            "ok": false,
            "message": format!(
                "No API key is configured for FIM model '{}'. Save the key in Model Settings.",
                model_name
            )
        }));
    }

    // 5. Model ID: user config value → model_name as fallback
    let model = if model.is_empty() {
        model_name.to_string()
    } else {
        model
    };

    let client = loom_inference::engine::build_http_client();
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
    let mut messages: Vec<serde_json::Value> = messages_raw
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

    // Use the orchestrator's active model to resolve config; an explicit
    // `model` param (e.g. write-mode inline edit passing the writing model)
    // takes precedence over the globally active model.
    let model_override = p
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let active_name = match model_override {
        Some(name) => Some(name),
        None => state.orchestrator.active_model_name().await,
    };

    let (model_id, base_url, api_key, context_size) = if let Some(ref name) = active_name {
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
        (model, url, key, config.context_size)
    } else {
        return Ok(serde_json::json!({ "ok": false, "message": "no active model configured" }));
    };

    // Clamp oversized prompts for small-context models (local inference):
    // without this, a multi-KB inline-edit prompt hits a 4K-8K window and the
    // provider hard-fails with a context-overflow error.
    if let Some(budget) = prompt_char_budget(context_size, max_tokens) {
        let total: usize = messages
            .iter()
            .map(|m| m["content"].as_str().unwrap_or("").chars().count())
            .sum();
        if total > budget {
            let over = total - budget;
            if let Some(longest) = messages
                .iter_mut()
                .max_by_key(|m| m["content"].as_str().unwrap_or("").chars().count())
            {
                let content = longest["content"].as_str().unwrap_or("").to_string();
                let target = content.chars().count().saturating_sub(over);
                match clamp_content_for_budget(&content, target) {
                    Some(clamped) => {
                        tracing::warn!(
                            before = content.chars().count(),
                            after = clamped.chars().count(),
                            budget,
                            "completion.chat prompt clamped to context budget"
                        );
                        longest["content"] = Value::String(clamped);
                    }
                    // EDIT_SCOPE 本身超预算——截断它会导致改写结果只覆盖选区一部分，
                    // 落地即丢用户原文。宁可明确报错也不发残缺 prompt。
                    None => {
                        return Ok(serde_json::json!({
                            "ok": false,
                            "message": "选区过大，超出当前模型的上下文窗口；请缩小选区，或在模型设置中增大 context_size"
                        }));
                    }
                }
            }
        }
    }

    let client = build_http_client();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_none_when_context_unset() {
        assert_eq!(prompt_char_budget(0, 1024), None);
    }

    #[test]
    fn budget_scales_with_context_minus_output() {
        // (8192 - 1024 - 256) * 3/4 = 5184
        assert_eq!(prompt_char_budget(8192, 1024), Some(5184));
        // Tiny window: saturates at 0 rather than underflowing.
        assert_eq!(prompt_char_budget(100, 4096), Some(0));
    }

    #[test]
    fn clamp_short_content_unchanged() {
        let s = "短内容";
        assert_eq!(clamp_content_for_budget(s, 100).as_deref(), Some(s));
    }

    #[test]
    fn clamp_plain_middle_elides() {
        let s = "a".repeat(1000);
        let out = clamp_content_for_budget(&s, 300).unwrap();
        assert!(out.contains("[已截断]"));
        assert!(out.chars().count() <= 300 + 32);
        assert!(out.starts_with('a'));
    }

    #[test]
    fn clamp_preserves_edit_scope() {
        let prefix = "前文".repeat(500); // 1000 chars
        let scope = "<<<EDIT_SCOPE\n被选中的文字\n";
        let suffix = "<<<SUFFIX\n".to_string() + &"后文".repeat(500);
        let content = format!("<<<PREFIX\n{prefix}{scope}{suffix}");
        let out = clamp_content_for_budget(&content, 200).unwrap();
        // EDIT_SCOPE 段必须完整保留（它是被改写的正文）
        assert!(out.contains("被选中的文字"));
        assert!(out.contains("[已截断]"));
    }

    #[test]
    fn clamp_oversized_edit_scope_returns_none() {
        // EDIT_SCOPE 本身超过预算时绝不截断正文，返回 None 让调用方报错
        let scope = format!("<<<EDIT_SCOPE\n{}\n", "长".repeat(500));
        let content = format!("<<<PREFIX\n短前文\n{scope}<<<SUFFIX\n短后文");
        assert_eq!(clamp_content_for_budget(&content, 100), None);
    }

    #[test]
    fn clamp_tiny_budget_tail_keeps() {
        let s = "abcdefghij";
        let out = clamp_content_for_budget(s, 5).unwrap();
        assert_eq!(out, "fghij");
    }
}

