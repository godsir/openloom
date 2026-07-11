//! Model dispatch handlers — model.list / model.switch / model.config.* / model.save_key /
//! model.check_key / model.discover

use loom_inference::engine::build_http_client;
use loom_types::{ErrorCode, JsonRpcError, ModelConfig};
use serde_json::{Value, json};

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "model.list" => Some(handle_model_list(state).await),
        "model.switch" => Some(handle_model_switch(state, p).await),
        "model.config.list" => Some(handle_model_config_list(state).await),
        "model.config.get" => Some(handle_model_config_get(state, p).await),
        "model.config.create" => Some(handle_model_config_create(state, p).await),
        "model.config.update" => Some(handle_model_config_update(state, p).await),
        "model.config.delete" => Some(handle_model_config_delete(state, p).await),
        "model.config.set_active" => Some(handle_model_config_set_active(state, p).await),
        "model.save_key" => Some(handle_model_save_key(state, p).await),
        "model.check_key" => Some(handle_model_check_key(state, p).await),
        "model.discover" => Some(handle_model_discover(state, p).await),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// model.list
// ---------------------------------------------------------------------------

async fn handle_model_list(state: &AppState) -> Result<Value, JsonRpcError> {
    let configs = state.orchestrator.model_config_list().await;
    let active = state.orchestrator.active_model_name().await;
    let models: Vec<Value> = configs
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "model": c.model,
                "backend": c.backend.name(),
                "backend_label": c.backend_label,
                "base_url": c.base_url,
                "is_active": active.as_deref() == Some(&c.name),
                "context_size": c.context_size,
                "capabilities": c.capabilities,
                "api_format": c.api_format,
                "api_key_env": c.api_key_env,
                "input_price": c.input_price,
                "output_price": c.output_price,
                "cache_read_price": c.cache_read_price,
                "cache_write_price": c.cache_write_price,
            })
        })
        .collect();
    Ok(json!({ "models": models, "activeModel": active }))
}

// ---------------------------------------------------------------------------
// model.switch
// ---------------------------------------------------------------------------

async fn handle_model_switch(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("model").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "model name required"));
    }
    state
        .orchestrator
        .model_config_set_active(name)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true, "model": name }))
}

// ---------------------------------------------------------------------------
// model.config.list
// ---------------------------------------------------------------------------

async fn handle_model_config_list(state: &AppState) -> Result<Value, JsonRpcError> {
    let configs = state.orchestrator.model_config_list().await;
    Ok(serde_json::to_value(configs).unwrap_or(json!([])))
}

// ---------------------------------------------------------------------------
// model.config.get
// ---------------------------------------------------------------------------

async fn handle_model_config_get(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    state
        .orchestrator
        .model_config_get(name)
        .await
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))
}

// ---------------------------------------------------------------------------
// model.config.create
// ---------------------------------------------------------------------------

async fn handle_model_config_create(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let config: ModelConfig = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    if config.name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    state
        .orchestrator
        .model_config_create(config)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// model.config.update
// ---------------------------------------------------------------------------

async fn handle_model_config_update(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    let prev_name = p
        .get("prev_name")
        .and_then(|v| v.as_str())
        .unwrap_or(name)
        .to_string();
    // Merge-update: load existing config by prev_name, apply only the
    // fields present in the request so a partial update never overwrites
    // base_url / model / api_format with serde defaults.
    let existing = state
        .orchestrator
        .model_config_get(&prev_name)
        .await
        .unwrap_or_else(|_| ModelConfig {
            name: prev_name.clone(),
            ..Default::default()
        });
    // Build merged config: start from existing, override with provided fields
    let merged = ModelConfig {
        name: name.to_string(),
        model: p
            .get("model")
            .and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_str().map(|s| s.to_string())
                }
            })
            .or(existing.model),
        model_type: serde_json::from_value(p.get("model_type").cloned().unwrap_or_default())
            .unwrap_or(existing.model_type),
        backend: serde_json::from_value(p.get("backend").cloned().unwrap_or_default())
            .unwrap_or(existing.backend),
        backend_label: p
            .get("backend_label")
            .and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_str().map(|s| s.to_string())
                }
            })
            .or(existing.backend_label),
        path: p
            .get("path")
            .and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_str().map(|s| s.to_string())
                }
            })
            .or(existing.path),
        base_url: p
            .get("base_url")
            .and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_str().map(|s| s.to_string())
                }
            })
            .or(existing.base_url),
        api_key_env: p
            .get("api_key_env")
            .and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_str().map(|s| s.to_string())
                }
            })
            .or(existing.api_key_env),
        api_format: p
            .get("api_format")
            .and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_str().map(|s| s.to_string())
                }
            })
            .or(existing.api_format),
        context_size: p
            .get("context_size")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(existing.context_size),
        max_output_tokens: p
            .get("max_output_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .or(existing.max_output_tokens),
        n_gpu_layers: p
            .get("n_gpu_layers")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(existing.n_gpu_layers),
        capabilities: serde_json::from_value(p.get("capabilities").cloned().unwrap_or_default())
            .unwrap_or(existing.capabilities),
        input_price: p
            .get("input_price")
            .and_then(|v| v.as_f64())
            .unwrap_or(existing.input_price),
        output_price: p
            .get("output_price")
            .and_then(|v| v.as_f64())
            .unwrap_or(existing.output_price),
        cache_read_price: p
            .get("cache_read_price")
            .and_then(|v| v.as_f64())
            .unwrap_or(existing.cache_read_price),
        cache_write_price: p
            .get("cache_write_price")
            .and_then(|v| v.as_f64())
            .unwrap_or(existing.cache_write_price),
    };
    state
        .orchestrator
        .model_config_update(merged, &prev_name)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// model.config.delete
// ---------------------------------------------------------------------------

async fn handle_model_config_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    state
        .orchestrator
        .model_config_delete(name)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// model.config.set_active
// ---------------------------------------------------------------------------

async fn handle_model_config_set_active(
    state: &AppState,
    p: &Value,
) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    state
        .orchestrator
        .model_config_set_active(name)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// model.save_key
// ---------------------------------------------------------------------------

async fn handle_model_save_key(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let backend = p.get("backend").and_then(|v| v.as_str()).unwrap_or("");
    let api_key = p.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
    let api_key_env = p.get("api_key_env").and_then(|v| v.as_str());
    if backend.is_empty() || api_key.is_empty() {
        return Err(err(
            ErrorCode::InvalidRequest,
            "backend and api_key required",
        ));
    }
    let env_name = api_key_env
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}_API_KEY", backend.to_uppercase().replace('-', "_")));

    // Store the key in the in-memory key_store and persist to disk.
    // This replaces the unsafe std::env::set_var approach.
    crate::credential::save_key(&state.data_dir, &state.key_store, &env_name, api_key).await;

    // Also update the orchestrator's key_store so it can resolve keys
    // when building cloud inference clients.
    {
        let ks_arc = state.orchestrator.key_store_arc();
        let mut ks = ks_arc.write().await;
        ks.insert(env_name.clone(), api_key.to_string());
    }

    // Persist the env var name in every matching model config's api_key_env field.
    let all_configs = state.orchestrator.model_config_list().await;
    let backend_val = backend.to_string();
    for cfg in all_configs {
        let matches = if api_key_env.is_some() {
            cfg.api_key_env.as_deref() == Some(&env_name)
                || cfg.api_key_env.as_deref() == api_key_env
        } else {
            format!("{:?}", cfg.backend).to_lowercase() == backend_val.to_lowercase()
        };
        if matches {
            let prev_name = cfg.name.clone();
            let updated = ModelConfig {
                api_key_env: Some(env_name.clone()),
                ..cfg
            };
            let _ = state
                .orchestrator
                .model_config_update(updated, &prev_name)
                .await;
        }
    }

    Ok(json!({ "ok": true, "env_name": env_name }))
}

// ---------------------------------------------------------------------------
// model.check_key
// ---------------------------------------------------------------------------

async fn handle_model_check_key(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let backend = p.get("backend").and_then(|v| v.as_str()).unwrap_or("");
    let api_key_env = p.get("api_key_env").and_then(|v| v.as_str());
    let env_name = api_key_env
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}_API_KEY", backend.to_uppercase().replace('-', "_")));
    // Read from in-memory key_store, fall back to OS env var
    let guard = state.key_store.read().await;
    let has_key = guard.get(&env_name).map(|v| !v.is_empty()).unwrap_or(false)
        || std::env::var(&env_name)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
        || api_key_env.map(|v| !v.is_empty()).unwrap_or(false);
    Ok(json!({ "set": has_key, "env_name": env_name }))
}

// ---------------------------------------------------------------------------
// model.discover
// ---------------------------------------------------------------------------

async fn handle_model_discover(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let backend = p.get("backend").and_then(|v| v.as_str()).unwrap_or("");
    let base_url = p.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
    let api_format = p
        .get("api_format")
        .and_then(|v| v.as_str())
        .unwrap_or("openai");
    let api_key_env = p.get("api_key_env").and_then(|v| v.as_str());
    if base_url.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "base_url required"));
    }
    // Resolve API key: try in-memory key_store first, then OS env vars
    let guard = state.key_store.read().await;
    let api_key = api_key_env
        .and_then(|raw| {
            if let Some(val) = guard.get(raw) {
                return Some(val.clone());
            }
            let looks_like_env_var = !raw.is_empty()
                && raw
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if !looks_like_env_var && !raw.is_empty() {
                Some(raw.to_string())
            } else {
                // Fall back to OS env var
                std::env::var(raw).ok().filter(|v| !v.is_empty())
            }
        })
        .or_else(|| {
            let auto_env = format!("{}_API_KEY", backend.to_uppercase().replace('-', "_"));
            guard
                .get(&auto_env)
                .cloned()
                .or_else(|| std::env::var(&auto_env).ok().filter(|v| !v.is_empty()))
        })
        .or_else(|| {
            let auto_env = match backend.to_lowercase().as_str() {
                "deepseek" => "DEEPSEEK_API_KEY",
                "openai" => "OPENAI_API_KEY",
                "anthropic" => "ANTHROPIC_API_KEY",
                _ => "OPENLOOM_API_KEY",
            };
            guard
                .get(auto_env)
                .cloned()
                .or_else(|| std::env::var(auto_env).ok().filter(|v| !v.is_empty()))
        })
        .unwrap_or_default();
    drop(guard);
    let client = build_http_client();

    // Standard OpenAI-compatible /models endpoint
    let url = if api_format == "anthropic" {
        format!("{}/v1/models", base_url.trim_end_matches('/'))
    } else {
        format!("{}/models", base_url.trim_end_matches('/'))
    };
    let req = if api_format == "anthropic" {
        client
            .get(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
    } else {
        client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
    };
    let resp = req
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &format!("HTTP error: {}", e)))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(err(
            ErrorCode::InternalError,
            &format!("API returned {}: {}", status, body),
        ));
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &format!("Parse error: {}", e)))?;
    let raw_models: Vec<Value> = body
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_else(|| {
            // Some providers (e.g. Ollama native) use "models" instead of "data"
            body.get("models")
                .and_then(|d| d.as_array())
                .cloned()
                .unwrap_or_default()
        });

    // Try native API for local providers — yields accurate context_length
    let native_ctx: std::collections::HashMap<String, u64> =
        if backend == "lmstudio" || backend == "LmStudio" {
            let native_url = format!(
                "{}/api/v1/models",
                base_url.trim_end_matches("/v1").trim_end_matches('/')
            );
            match client
                .get(&native_url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => match resp.json::<Value>().await {
                    Ok(v) => v
                        .get("data")
                        .and_then(|d| d.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|m| {
                                    let id = m.get("id").and_then(|v| v.as_str())?;
                                    let ctx = m
                                        .get("max_context_length")
                                        .and_then(json_value_as_u64)
                                        .filter(|&n| n > 0);
                                    Some((id.to_string(), ctx.unwrap_or(0)))
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    Err(_) => std::collections::HashMap::new(),
                },
                _ => std::collections::HashMap::new(),
            }
        } else if backend == "ollama" || backend == "Ollama" {
            // Ollama native /api/tags — yields model names then /api/show for context
            let ollama_host = base_url.trim_end_matches("/v1").trim_end_matches('/');
            let mut ctx_map = std::collections::HashMap::new();
            if let Ok(resp) = client
                .get(format!("{}/api/tags", ollama_host))
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
                && resp.status().is_success()
                && let Ok(v) = resp.json::<Value>().await
                && let Some(models) = v.get("models").and_then(|d| d.as_array())
            {
                for m in models {
                    if let Some(name) = m.get("name").and_then(|v| v.as_str()) {
                        // Try /api/show for detailed model_info
                        if let Ok(show_resp) = client
                            .post(format!("{}/api/show", ollama_host))
                            .json(&json!({ "name": name }))
                            .timeout(std::time::Duration::from_secs(3))
                            .send()
                            .await
                            && show_resp.status().is_success()
                            && let Ok(info) = show_resp.json::<Value>().await
                        {
                            let ctx = info
                                .get("model_info")
                                .and_then(|mi| {
                                    mi.get("llama.context_length")
                                        .or_else(|| mi.get("context_length"))
                                        .or_else(|| mi.get("max_context_length"))
                                })
                                .and_then(json_value_as_u64);
                            if let Some(c) = ctx.filter(|&n| n > 0) {
                                ctx_map.insert(name.to_string(), c);
                            }
                        }
                    }
                }
            }
            ctx_map
        } else {
            std::collections::HashMap::new()
        };

    let models: Vec<Value> = raw_models
        .iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(|v| v.as_str())?;
            // Resolve context length: native API → standard API fields → known lookup
            let ctx = native_ctx
                .get(id)
                .copied()
                .filter(|&n| n > 0)
                .or_else(|| {
                    item.get("context_window")
                        .or_else(|| item.get("context_length"))
                        .or_else(|| item.get("max_input_tokens"))
                        .or_else(|| item.get("max_context_length"))
                        .and_then(json_value_as_u64)
                        .filter(|&n| n > 0)
                })
                .or_else(|| known_context_window(id));
            Some(json!({ "id": id, "context_length": ctx }))
        })
        .collect();
    Ok(json!({ "models": models }))
}

// ---------------------------------------------------------------------------
// Shared helpers for model.discover
// ---------------------------------------------------------------------------

/// Parse a JSON value as u64, handling both numeric and string representations.
fn json_value_as_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
}

/// Fallback lookup for well-known model context windows when the provider's
/// API does not return one. Patterns are sorted longest-first so specific
/// model names match before generic family prefixes.
fn known_context_window(model_id: &str) -> Option<u64> {
    let patterns: &[(&str, u64)] = &[
        // OpenAI / compatible
        ("gpt-4.1", 1_000_000),
        ("gpt-4o-mini", 128_000),
        ("gpt-4o", 128_000),
        ("gpt-4-turbo", 128_000),
        ("gpt-4", 8_192),
        ("gpt-3.5-turbo", 16_385),
        ("o4-mini", 200_000),
        ("o3-mini", 200_000),
        ("o3", 200_000),
        ("o1-mini", 200_000),
        ("o1", 200_000),
        // Anthropic
        ("claude-opus-4-7", 200_000),
        ("claude-opus-4", 200_000),
        ("claude-sonnet-4", 200_000),
        ("claude-haiku-4", 200_000),
        ("claude-3.5", 200_000),
        ("claude-3", 200_000),
        ("claude", 200_000),
        // DeepSeek
        ("deepseek-r1", 128_000),
        ("deepseek-v4", 128_000),
        ("deepseek-v3", 128_000),
        ("deepseek", 64_000),
        // Gemini
        ("gemini-2.5", 1_000_000),
        ("gemini-2.0-flash", 1_000_000),
        ("gemini-2.0", 1_000_000),
        ("gemini-1.5", 2_000_000),
        ("gemini", 32_000),
        // Mistral
        ("mistral-large", 128_000),
        ("mistral-small", 32_000),
        ("mistral", 32_000),
        // Llama
        ("llama-4", 128_000),
        ("llama-3.3", 128_000),
        ("llama-3.2", 128_000),
        ("llama-3.1", 128_000),
        ("llama-3", 8_192),
        ("llama-2", 4_096),
        ("llama", 4_096),
        // Qwen
        ("qwen3", 128_000),
        ("qwen2.5", 128_000),
        ("qwen2", 32_000),
        ("qwen", 32_000),
        // Yi
        ("yi-large", 200_000),
        ("yi", 32_000),
        // Command R
        ("command-r", 128_000),
        // Phi
        ("phi-4", 128_000),
        ("phi-3", 128_000),
        ("phi", 4_096),
        // GLM
        ("glm-4", 128_000),
        ("glm", 8_000),
        // InternLM
        ("internlm", 32_000),
        // MiniMax
        ("minimax", 128_000),
        // Moonshot / Kimi
        ("moonshot", 128_000),
        ("kimi", 128_000),
        // DBRX / Databricks
        ("dbrx", 32_000),
    ];

    let lower = model_id.to_lowercase();
    // Longest patterns first so "gpt-4-turbo" matches before "gpt-4"
    let mut sorted: Vec<&(&str, u64)> = patterns.iter().collect();
    sorted.sort_by_key(|(p, _)| -(p.len() as i32));
    for (pattern, ctx) in sorted {
        if lower.contains(pattern) {
            return Some(*ctx);
        }
    }
    None
}
