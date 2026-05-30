//! JSON-RPC 2.0 dispatch — routes incoming requests to orchestrator methods.
//!
//! Supported: system, agent, chat, session, config, mcp, lsp, tools, skills, plugins.

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use loom_types::Message as LoomMessage;
use loom_types::{
    AgentConfig, ContentPart, ErrorCode, JsonRpcError, JsonRpcRequest, JsonRpcResponse, ModelConfig,
};
use lume_mcp::McpServerConfig;
use lume_skills::SkillLoader;
use serde_json::{Value, json};
use tokio::sync::RwLock;

use crate::AppState;

#[derive(Default)]
pub struct SessionStore {
    sessions: RwLock<HashMap<String, SessionData>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionData {
    pub id: String,
    pub created_at: String,
    pub message_count: usize,
    pub title: Option<String>,
    pub messages: Vec<LoomMessage>,
    #[serde(default)]
    pub agent_config_name: Option<String>,
}

impl SessionStore {
    pub async fn list(&self) -> Vec<SessionData> {
        self.sessions.read().await.values().cloned().collect()
    }

    pub async fn create(&self, cwd: Option<&str>) -> SessionData {
        let id = uuid::Uuid::now_v7().to_string();
        let session = SessionData {
            id: id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            message_count: 0,
            title: cwd.map(|s| s.to_string()),
            messages: Vec::new(),
            agent_config_name: None,
        };
        self.sessions
            .write()
            .await
            .insert(id.clone(), session.clone());
        session
    }

    pub async fn get(&self, id: &str) -> Option<SessionData> {
        self.sessions.read().await.get(id).cloned()
    }

    pub async fn get_or_create(&self, path: Option<&str>) -> SessionData {
        if let Some(id) = path
            && let Some(s) = self.get(id).await
        {
            return s;
        }
        self.create(path).await
    }

    pub async fn add_message(&self, session_id: &str, role: &str, content: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(s) = sessions.get_mut(session_id) {
            let msg = match role {
                "user" => LoomMessage::user(content),
                "assistant" => LoomMessage::assistant(content),
                _ => LoomMessage::user(content),
            };
            s.messages.push(msg);
            s.message_count = s.messages.len();
        }
    }

    pub async fn rename(&self, id: &str, title: &str) -> bool {
        if let Some(s) = self.sessions.write().await.get_mut(id) {
            s.title = Some(title.to_string());
            true
        } else {
            false
        }
    }

    pub async fn delete(&self, id: &str) -> bool {
        self.sessions.write().await.remove(id).is_some()
    }

    /// Restore a persisted session on startup.
    pub async fn restore(
        &self,
        id: String,
        created_at: String,
        message_count: usize,
        title: Option<String>,
    ) {
        self.sessions.write().await.insert(
            id.clone(),
            SessionData {
                id,
                created_at,
                message_count,
                title,
                messages: Vec::new(),
                agent_config_name: None,
            },
        );
    }

    pub async fn bind_agent(
        &self,
        session_id: &str,
        agent_config_name: &str,
    ) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        match sessions.get_mut(session_id) {
            Some(s) => {
                s.agent_config_name = Some(agent_config_name.to_string());
                Ok(())
            }
            None => Err("session not found".to_string()),
        }
    }

    pub async fn get_bound_agent(&self, session_id: &str) -> Option<String> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .and_then(|s| s.agent_config_name.clone())
    }
}

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

pub async fn dispatch_handler(state: Arc<AppState>, req: JsonRpcRequest) -> JsonRpcResponse {
    let result = dispatch_method(&state, &req).await;
    let (result_val, error_val) = match result {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    };
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        result: result_val,
        error: error_val,
        id: req.id,
    }
}

pub async fn dispatch_method(
    state: &AppState,
    req: &JsonRpcRequest,
) -> Result<Value, JsonRpcError> {
    let p = req.params.clone().unwrap_or(json!({}));

    match req.method.as_str() {
        // === System ===
        "system.health" => Ok(json!({
            "status": "ok", "version": "0.2.17",
            "agent_count": state.orchestrator.list_agents().await.len(),
            "tool_count": state.orchestrator.tool_registry().await.len(),
        })),

        // === Chat ===
        "chat.send" => {
            let content = p.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let attached_files_count = p
                .get("attached_files")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            tracing::info!(
                content_len = content.len(),
                attached_files_count,
                session_id = %p.get("session_id").and_then(|v| v.as_str()).unwrap_or("default"),
                "[dispatch] chat.send received"
            );
            let attached_images = parse_attached_images(&p);
            if !attached_images.is_empty() {
                tracing::info!(
                    image_count = attached_images.len(),
                    "parsed attached images"
                );
            }
            // Read text-based non-image files and inject their content
            let file_contents = parse_attached_file_contents(&p);
            let combined_content = if file_contents.is_empty() {
                content.to_string()
            } else {
                let mut combined = content.to_string();
                combined.push_str(&file_contents);
                combined
            };
            if combined_content.is_empty() && attached_images.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "content required"));
            }
            let session_id = p
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("default");

            // Optional per-message model override
            let model_override = p.get("model").and_then(|v| v.as_str());

            // Thinking level: off/auto/low/medium/high → budget
            let thinking_level = p
                .get("thinking_level")
                .and_then(|v| v.as_str())
                .unwrap_or("off");
            let thinking_budget: Option<usize> = match thinking_level {
                "low" => Some(2048),
                "medium" | "mid" => Some(8192),
                "high" => Some(32768),
                "auto" => Some(16384),
                _ => None, // "off" or unknown
            };

            // If model is explicitly provided and differs from active, switch it
            if let Some(model_name) = model_override {
                let active = state.orchestrator.active_model_name().await;
                if active.as_deref() != Some(model_name) {
                    let _ = state.orchestrator.model_config_set_active(model_name).await;
                }
            }

            // Parse selected skills
            let selected_skills: Vec<String> = p
                .get("skills")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            // Resolve agent config for this session
            let config_name = state
                .sessions
                .get_bound_agent(session_id)
                .await
                .unwrap_or_else(|| "default".to_string());
            let agent_config = state
                .orchestrator
                .agent_config_get(&config_name)
                .await
                .unwrap_or_default();

            let result = state
                .orchestrator
                .process_message_with_config(
                    &combined_content,
                    session_id,
                    &agent_config,
                    thinking_budget,
                    attached_images,
                    selected_skills,
                )
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            state
                .sessions
                .add_message(session_id, "user", content)
                .await;
            state
                .sessions
                .add_message(session_id, "assistant", &result.response)
                .await;
            Ok(json!({
                "response": result.response,
                "session_id": session_id,
                "tool_calls": result.tool_calls_made,
                "iterations": result.iterations,
                "tokens": result.prompt_tokens + result.completion_tokens,
            }))
        }

        "chat.stop" => {
            let session_id = p
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            let killed = state
                .orchestrator
                .stop_session(session_id)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true, "killed": killed }))
        }

        // === Agent ===
        "agent.list" => Ok(json!({ "agents": state.orchestrator.list_agents().await })),
        "agent.status" => {
            let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
            state
                .orchestrator
                .agent_status(&loom_types::AgentId::from(id))
                .await
                .map(|s| serde_json::to_value(s).unwrap_or_default())
                .map_err(|e| err(ErrorCode::AgentNotFound, &e.to_string()))
        }
        "agent.kill" => {
            let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
            state
                .orchestrator
                .kill_agent(&loom_types::AgentId::from(id))
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "agent.config.list" => {
            let configs = state.orchestrator.agent_config_list().await;
            Ok(json!({ "configs": configs }))
        }
        "agent.config.get" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("default");
            let config = state
                .orchestrator
                .agent_config_get(name)
                .await
                .map_err(|e| err(ErrorCode::AgentNotFound, &e.to_string()))?;
            Ok(serde_json::to_value(config).unwrap_or_default())
        }
        "agent.config.create" => {
            let config: AgentConfig = serde_json::from_value(p.clone())
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            if config.name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            state
                .orchestrator
                .agent_config_create(config)
                .await
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "agent.config.update" => {
            let config: AgentConfig = serde_json::from_value(p.clone())
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            if config.name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            let new_name = config.name.clone();
            let prev_name = p
                .get("prev_name")
                .and_then(|v| v.as_str())
                .unwrap_or(&new_name)
                .to_string();
            let cache_size = state.orchestrator.agent_config_list().await.len();
            tracing::info!(
                new_name = %new_name,
                prev_name = %prev_name,
                cache_size,
                "agent.config.update"
            );
            state
                .orchestrator
                .agent_config_update(config, &prev_name)
                .await
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "agent.config.delete" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            if name == "default" {
                return Err(err(
                    ErrorCode::InvalidRequest,
                    "cannot delete default config",
                ));
            }
            state
                .orchestrator
                .agent_config_delete(name)
                .await
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }

        // === Session ===
        "session.list" => Ok(json!({ "sessions": state.sessions.list().await })),
        "session.create" => {
            let cwd = p.get("cwd").and_then(|v| v.as_str());
            let s = state.sessions.create(cwd).await;
            state.orchestrator.ensure_session_persisted(&s.id).await;
            Ok(json!({ "session_id": s.id, "path": s.id, "created_at": s.created_at }))
        }
        "session.switch" => {
            let id = p
                .get("session_id")
                .or_else(|| p.get("path"))
                .and_then(|v| v.as_str());
            let s = state.sessions.get_or_create(id).await;
            Ok(json!({ "session_id": s.id, "path": s.id, "title": s.title }))
        }
        "session.messages" => {
            let id = p
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            let s = state.sessions.get_or_create(Some(id)).await;
            // Always prefer orchestrator history (rich ContentParts) over SessionStore (legacy text-only)
            let history = state.orchestrator.session_history(id).await;
            let msgs = if !history.is_empty() {
                tracing::info!(
                    session_id = %id,
                    history_len = history.len(),
                    "session.messages: returning orchestrator history"
                );
                history
            } else {
                // Try loading from persisted DB if orchestrator cache is empty
                match state.orchestrator.load_history(id).await {
                    Ok(_) => tracing::info!(session_id = %id, "load_history ok"),
                    Err(e) => tracing::warn!(session_id = %id, error = %e, "load_history failed"),
                }
                let loaded = state.orchestrator.session_history(id).await;
                if !loaded.is_empty() {
                    tracing::info!(
                        session_id = %id,
                        loaded_len = loaded.len(),
                        "session.messages: loaded from DB"
                    );
                    loaded
                } else {
                    tracing::info!(
                        session_id = %id,
                        legacy_msgs = s.messages.len(),
                        "session.messages: fallback to SessionStore"
                    );
                    s.messages
                }
            };
            tracing::info!(
                session_id = %id,
                returning = msgs.len(),
                "session.messages: returning to client"
            );
            Ok(json!({ "messages": msgs, "hasMore": false }))
        }
        "session.delete_message" => {
            let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
            let index = p.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if session_id.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "session_id required"));
            }
            state
                .orchestrator
                .delete_message(session_id, index)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "session.rename" => {
            let id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
            let title = p.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let ok = state.sessions.rename(id, title).await;
            if ok {
                state.orchestrator.rename_session_persisted(id, title).await;
            }
            Ok(json!({ "ok": ok }))
        }
        "session.auto_title" => {
            let id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "session_id required"));
            }
            match state.orchestrator.auto_title(id).await {
                Ok(title) => {
                    let _ = state.sessions.rename(id, &title).await;
                    state.orchestrator.rename_session_persisted(id, &title).await;
                    Ok(json!({ "title": title }))
                }
                Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
            }
        }
        "session.delete" => {
            let id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
            let ok = state.sessions.delete(id).await;
            if ok {
                state.orchestrator.delete_session_persisted(id).await;
            }
            Ok(json!({ "ok": ok }))
        }
        "session.bind_agent" => {
            let session_id = p
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            let config_name = p
                .get("agent_config_name")
                .and_then(|v| v.as_str())
                .unwrap_or("default");

            // Verify the config exists
            let _ = state
                .orchestrator
                .agent_config_get(config_name)
                .await
                .map_err(|_e| {
                    err(
                        ErrorCode::InvalidRequest,
                        &format!("agent config '{}' not found", config_name),
                    )
                })?;

            state
                .sessions
                .bind_agent(session_id, config_name)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e))?;
            Ok(json!({ "ok": true }))
        }

        // === Config ===
        "config.get" => {
            let key = p.get("key").and_then(|v| v.as_str());
            Ok(json!({ "config": { "key": key } }))
        }
        "config.set" => Ok(json!({ "ok": true })),

        // === Model ===
        "model.list" => {
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
        "model.switch" => {
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
        "model.config.list" => {
            let configs = state.orchestrator.model_config_list().await;
            Ok(serde_json::to_value(configs).unwrap_or(json!([])))
        }
        "model.config.get" => {
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
        "model.config.create" => {
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
        "model.config.update" => {
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
                model_type: serde_json::from_value(
                    p.get("model_type").cloned().unwrap_or_default(),
                )
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
                capabilities: serde_json::from_value(
                    p.get("capabilities").cloned().unwrap_or_default(),
                )
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
        "model.config.delete" => {
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
        "model.config.set_active" => {
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

        "model.save_key" => {
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
            // Set in current process immediately
            unsafe {
                std::env::set_var(&env_name, api_key);
            }
            // Try OS-level persistence as secondary backup (non-fatal)
            crate::save_credential(&env_name, api_key);

            // Primary persistence: write the literal API key into every matching
            // model config's api_key_env field in the DB. This survives restarts
            // without depending on the OS env var (setx/profile) mechanism.
            let all_configs = state.orchestrator.model_config_list().await;
            let backend_val = backend.to_string();
            for cfg in all_configs {
                let matches = if api_key_env.is_some() {
                    // Custom provider: match by the configured env var name
                    cfg.api_key_env.as_deref() == Some(&env_name)
                        || cfg.api_key_env.as_deref() == api_key_env
                } else {
                    // Preset provider: match by backend type
                    format!("{:?}", cfg.backend).to_lowercase() == backend_val.to_lowercase()
                };
                if matches {
                    // Merge-update: only change api_key_env, keep all other fields.
                    // Store the environment variable NAME (e.g. "OPENAI_API_KEY"),
                    // NOT the actual API key value. The key itself lives in the OS
                    // environment variable or credential store.
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

        "model.check_key" => {
            let backend = p.get("backend").and_then(|v| v.as_str()).unwrap_or("");
            let api_key_env = p.get("api_key_env").and_then(|v| v.as_str());
            let env_name = api_key_env
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}_API_KEY", backend.to_uppercase().replace('-', "_")));
            // Check if env var is set, OR the raw value itself is a non-empty literal key
            let has_key = std::env::var(&env_name)
                .ok()
                .filter(|v| !v.is_empty())
                .is_some()
                || api_key_env.map(|v| !v.is_empty()).unwrap_or(false);
            Ok(json!({ "set": has_key, "env_name": env_name }))
        }

        "model.discover" => {
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
            let api_key = api_key_env
                .and_then(|raw| {
                    if let Ok(val) = std::env::var(raw) {
                        return Some(val);
                    }
                    let looks_like_env_var = !raw.is_empty()
                        && raw
                            .chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
                    if !looks_like_env_var && !raw.is_empty() {
                        Some(raw.to_string())
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    let env_name = format!("{}_API_KEY", backend.to_uppercase().replace('-', "_"));
                    std::env::var(&env_name).ok()
                })
                .or_else(|| {
                    let auto_env = match backend.to_lowercase().as_str() {
                        "deepseek" => "DEEPSEEK_API_KEY",
                        "openai" => "OPENAI_API_KEY",
                        "anthropic" => "ANTHROPIC_API_KEY",
                        _ => "OPENLOOM_API_KEY",
                    };
                    std::env::var(auto_env).ok()
                })
                .unwrap_or_default();
            let client = reqwest::Client::new();

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
            let native_ctx: std::collections::HashMap<String, u64> = if backend == "lmstudio"
                || backend == "LmStudio"
            {
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
                                            .and_then(|v| json_value_as_u64(v))
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
                {
                    if resp.status().is_success() {
                        if let Ok(v) = resp.json::<Value>().await {
                            if let Some(models) = v.get("models").and_then(|d| d.as_array()) {
                                for m in models {
                                    if let Some(name) = m.get("name").and_then(|v| v.as_str()) {
                                        // Try /api/show for detailed model_info
                                        if let Ok(show_resp) = client
                                            .post(format!("{}/api/show", ollama_host))
                                            .json(&json!({ "name": name }))
                                            .timeout(std::time::Duration::from_secs(3))
                                            .send()
                                            .await
                                        {
                                            if show_resp.status().is_success() {
                                                if let Ok(info) = show_resp.json::<Value>().await {
                                                    let ctx = info
                                                        .get("model_info")
                                                        .and_then(|mi| {
                                                            mi.get("llama.context_length")
                                                                .or_else(|| {
                                                                    mi.get("context_length")
                                                                })
                                                                .or_else(|| {
                                                                    mi.get("max_context_length")
                                                                })
                                                        })
                                                        .and_then(|v| json_value_as_u64(v));
                                                    if let Some(c) = ctx.filter(|&n| n > 0) {
                                                        ctx_map.insert(name.to_string(), c);
                                                    }
                                                }
                                            }
                                        }
                                    }
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
                                .and_then(|v| json_value_as_u64(v))
                                .filter(|&n| n > 0)
                        })
                        .or_else(|| known_context_window(id));
                    Some(json!({ "id": id, "context_length": ctx }))
                })
                .collect();
            Ok(json!({ "models": models }))
        }

        // === Vision Auxiliary Config ===
        "config.get_vision" => {
            let home = dirs::home_dir().unwrap_or_default().join(".loom");
            let config_file = home.join("vision.json");
            let config: Value = std::fs::read_to_string(&config_file)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(json!({ "enabled": false, "model": null }));
            Ok(config)
        }

        "config.set_vision" => {
            let enabled = p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            let model = p
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let home = dirs::home_dir().unwrap_or_default().join(".loom");
            let _ = std::fs::create_dir_all(&home);
            let config = json!({ "enabled": enabled, "model": model });
            let config_file = home.join("vision.json");
            std::fs::write(
                &config_file,
                serde_json::to_string_pretty(&config).unwrap_or_default(),
            )
            .map_err(|e| err(ErrorCode::InternalError, &format!("Write error: {}", e)))?;
            Ok(json!({ "ok": true }))
        }

        "config.get_auxiliary" => {
            let home = dirs::home_dir().unwrap_or_default().join(".loom");
            let config_file = home.join("auxiliary.json");
            let config: Value = std::fs::read_to_string(&config_file)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(json!({ "summary_model": null, "entity_model": null }));
            Ok(config)
        }

        "config.set_auxiliary" => {
            let summary_model = p
                .get("summary_model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let entity_model = p
                .get("entity_model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let home = dirs::home_dir().unwrap_or_default().join(".loom");
            let _ = std::fs::create_dir_all(&home);
            let config = json!({ "summary_model": summary_model, "entity_model": entity_model });
            let config_file = home.join("auxiliary.json");
            std::fs::write(
                &config_file,
                serde_json::to_string_pretty(&config).unwrap_or_default(),
            )
            .map_err(|e| err(ErrorCode::InternalError, &format!("Write error: {}", e)))?;
            Ok(json!({ "ok": true }))
        }

        // === MCP ===
        "mcp.list_servers" => {
            let names = state.orchestrator.mcp_client().server_names().await;
            Ok(json!({ "servers": names }))
        }
        "mcp.list_tools" => {
            let defs = state
                .orchestrator
                .mcp_client()
                .all_tool_definitions()
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "tools": defs }))
        }
        "mcp.list_resources" => {
            let server = p.get("server").and_then(|v| v.as_str()).unwrap_or("");
            if server.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "server required"));
            }
            let resources = state
                .orchestrator
                .mcp_client()
                .list_resources(server)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "resources": resources }))
        }
        "mcp.read_resource" => {
            let server = p.get("server").and_then(|v| v.as_str()).unwrap_or("");
            let uri = p.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            if server.is_empty() || uri.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "server and uri required"));
            }
            let contents = state
                .orchestrator
                .mcp_client()
                .read_resource(server, uri)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(serde_json::to_value(contents).unwrap_or_default())
        }
        "mcp.list_resource_templates" => {
            let server = p.get("server").and_then(|v| v.as_str()).unwrap_or("");
            if server.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "server required"));
            }
            let templates = state
                .orchestrator
                .mcp_client()
                .list_resource_templates(server)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "templates": templates }))
        }
        "mcp.list_prompts" => {
            let server = p.get("server").and_then(|v| v.as_str()).unwrap_or("");
            if server.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "server required"));
            }
            let prompts = state
                .orchestrator
                .mcp_client()
                .list_prompts(server)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "prompts": prompts }))
        }
        "mcp.get_prompt" => {
            let server = p.get("server").and_then(|v| v.as_str()).unwrap_or("");
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if server.is_empty() || name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "server and name required"));
            }
            let args = p.get("arguments");
            let result = state
                .orchestrator
                .mcp_client()
                .get_prompt(server, name, args)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(serde_json::to_value(result).unwrap_or_default())
        }
        "mcp.connect" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            let transport = p
                .get("transport")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");
            let persist = p.get("persist").and_then(|v| v.as_bool()).unwrap_or(true);
            let autostart = p.get("autostart").and_then(|v| v.as_bool()).unwrap_or(true);
            let config = McpServerConfig {
                name: name.to_string(),
                transport: transport.to_string(),
                command: p
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                args: p
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                url: p.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()),
                headers: p
                    .get("headers")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default(),
                env: p
                    .get("env")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default(),
                cwd: p.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string()),
                startup_timeout_secs: p
                    .get("startup_timeout_secs")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(30),
                tool_timeout_secs: p
                    .get("tool_timeout_secs")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(60),
                enabled_tools: p.get("enabled_tools").and_then(|v| v.as_array()).map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                }),
                disabled_tools: p.get("disabled_tools").and_then(|v| v.as_array()).map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                }),
            };
            // Persist before connect: even if the server fails to start, the
            // user's filled-in form values survive so they can edit + retry
            // without re-typing everything.
            if persist {
                if let Err(e) = state.orchestrator.save_mcp_server(&config, autostart).await {
                    tracing::warn!(error = %e, "failed to persist MCP server config");
                }
            }
            state
                .orchestrator
                .connect_mcp_server(config)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "mcp.disconnect" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            state
                .orchestrator
                .mcp_client()
                .disconnect(name)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "mcp.server_health" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            let healthy = state.orchestrator.mcp_client().server_health(name).await;
            Ok(json!({ "healthy": healthy }))
        }
        // === MCP saved (persisted) configs ===
        "mcp.config.list" => {
            let configs = state
                .orchestrator
                .list_saved_mcp_servers()
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            let live: std::collections::HashSet<String> = state
                .orchestrator
                .mcp_client()
                .server_names()
                .await
                .into_iter()
                .collect();
            let items: Vec<serde_json::Value> = configs
                .into_iter()
                .map(|(cfg, autostart)| {
                    let connected = live.contains(&cfg.name);
                    json!({
                        "name": cfg.name,
                        "transport": cfg.transport,
                        "command": cfg.command,
                        "args": cfg.args,
                        "url": cfg.url,
                        "headers": cfg.headers,
                        "env": cfg.env,
                        "cwd": cfg.cwd,
                        "startup_timeout_secs": cfg.startup_timeout_secs,
                        "tool_timeout_secs": cfg.tool_timeout_secs,
                        "enabled_tools": cfg.enabled_tools,
                        "disabled_tools": cfg.disabled_tools,
                        "autostart": autostart,
                        "connected": connected,
                    })
                })
                .collect();
            Ok(json!({ "configs": items }))
        }
        "mcp.config.save" => {
            // Save without connecting — used by the editor to update fields
            // on a disconnected entry, or to add an autostart entry for later.
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            let transport = p
                .get("transport")
                .and_then(|v| v.as_str())
                .unwrap_or("stdio");
            let autostart = p.get("autostart").and_then(|v| v.as_bool()).unwrap_or(true);
            let config = McpServerConfig {
                name: name.to_string(),
                transport: transport.to_string(),
                command: p
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                args: p
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                url: p.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()),
                headers: p
                    .get("headers")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default(),
                env: p
                    .get("env")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default(),
                cwd: p.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string()),
                startup_timeout_secs: p
                    .get("startup_timeout_secs")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(30),
                tool_timeout_secs: p
                    .get("tool_timeout_secs")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(60),
                enabled_tools: p.get("enabled_tools").and_then(|v| v.as_array()).map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                }),
                disabled_tools: p.get("disabled_tools").and_then(|v| v.as_array()).map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                }),
            };
            state
                .orchestrator
                .save_mcp_server(&config, autostart)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "mcp.config.delete" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            state
                .orchestrator
                .delete_saved_mcp_server(name)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }

        // === LSP ===
        "lsp.list_servers" => {
            let servers = state.orchestrator.lsp_client().list_servers().await;
            Ok(json!({ "servers": servers }))
        }
        "lsp.diagnostics" => {
            let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            if file_path.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "file_path required"));
            }
            let result = state
                .orchestrator
                .lsp_client()
                .diagnostics(file_path)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(result)
        }
        "lsp.completion" => {
            let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let character = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            if file_path.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "file_path required"));
            }
            let result = state
                .orchestrator
                .lsp_client()
                .completion(file_path, line, character)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(result)
        }
        "lsp.hover" => {
            let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let character = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            if file_path.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "file_path required"));
            }
            let result = state
                .orchestrator
                .lsp_client()
                .hover(file_path, line, character)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(result)
        }
        "lsp.definition" => {
            let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let character = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            if file_path.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "file_path required"));
            }
            let result = state
                .orchestrator
                .lsp_client()
                .definition(file_path, line, character)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(result)
        }
        "lsp.references" => {
            let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let character = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let include_decl = p
                .get("include_declaration")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if file_path.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "file_path required"));
            }
            let result = state
                .orchestrator
                .lsp_client()
                .references(file_path, line, character, include_decl)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(result)
        }
        "lsp.symbols" => {
            let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            if file_path.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "file_path required"));
            }
            let result = state
                .orchestrator
                .lsp_client()
                .document_symbols(file_path)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(result)
        }
        "lsp.shutdown" => {
            let language = p.get("language").and_then(|v| v.as_str()).unwrap_or("");
            if language.is_empty() {
                return Err(err(
                    ErrorCode::InvalidRequest,
                    "language required (e.g. 'rust', 'typescript')",
                ));
            }
            state
                .orchestrator
                .lsp_client()
                .shutdown(language)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "lsp.shutdown_all" => {
            state
                .orchestrator
                .lsp_client()
                .shutdown_all()
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "lsp.supported_languages" => {
            let langs = state.orchestrator.lsp_client().supported_languages();
            let list: Vec<Value> = langs
                .iter()
                .map(|(lang, cmd)| json!({ "language": lang, "command": cmd }))
                .collect();
            Ok(json!({ "languages": list }))
        }
        "lsp.start" => {
            let language = p.get("language").and_then(|v| v.as_str()).unwrap_or("");
            let command = p.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let args: Vec<String> = p
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if language.is_empty() || command.is_empty() {
                return Err(err(
                    ErrorCode::InvalidRequest,
                    "language and command required",
                ));
            }
            state
                .orchestrator
                .lsp_client()
                .start_custom(language, command, &args)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }

        // === Tools ===
        "tools.list" => {
            let names = state.orchestrator.tool_registry().await.list_names();
            Ok(json!({ "tools": names }))
        }

        // === Skills ===
        "skills.list" => {
            let home = dirs::home_dir().unwrap_or_default();
            let data_dir = home.join(".loom");
            let mut loader = SkillLoader::new();
            loader.add_standard_paths(&data_dir);
            let skills = loader.discover().unwrap_or_default();
            let list: Vec<Value> = skills
                .iter()
                .map(|s| {
                    json!({
                        "name": s.manifest.name,
                        "description": s.manifest.description,
                        "path": s.source_path.display().to_string(),
                        "version": s.manifest.version,
                        "user_invocable": s.manifest.user_invocable,
                        "always_active": s.manifest.always_active,
                    })
                })
                .collect();
            Ok(json!({ "skills": list }))
        }
        "skills.get" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            // Discover from disk — the orchestrator's in-memory map is only
            // populated by the CLI, not the server.
            let home = dirs::home_dir().unwrap_or_default();
            let data_dir = home.join(".loom");
            let mut loader = SkillLoader::new();
            loader.add_standard_paths(&data_dir);
            let skills = loader.discover().unwrap_or_default();
            match skills.iter().find(|s| s.manifest.name == name) {
                Some(skill) => Ok(json!({ "content": skill.body })),
                None => Err(err(
                    ErrorCode::MethodNotFound,
                    &format!("skill '{}' not found", name),
                )),
            }
        }
        "skills.import" => {
            // Import a skill: write files to ~/.loom/skills/<name>/
            // Accepts: { name: string, files: [{ path: string, content: string }] }
            // Minimum: at least one file named SKILL.md
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            let files = p.get("files").and_then(|v| v.as_array());
            if files.is_none() || files.unwrap().is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "files array required"));
            }
            let home = dirs::home_dir().unwrap_or_default();
            let skill_dir = home.join(".loom").join("skills").join(name);
            std::fs::create_dir_all(&skill_dir)
                .map_err(|e| err(ErrorCode::InternalError, &format!("mkdir failed: {}", e)))?;

            let mut wrote = 0usize;
            for file in files.unwrap() {
                let rel_path = file.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let content = file.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if rel_path.is_empty() {
                    continue;
                }
                // Prevent path traversal
                if rel_path.contains("..") {
                    continue;
                }
                let target = skill_dir.join(rel_path);
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(&target, content)
                    .map_err(|e| err(ErrorCode::InternalError, &format!("write failed: {}", e)))?;
                wrote += 1;
            }
            Ok(
                json!({ "ok": true, "path": skill_dir.display().to_string(), "files_written": wrote }),
            )
        }
        "skills.delete" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            let home = dirs::home_dir().unwrap_or_default();
            let skill_dir = home.join(".loom").join("skills").join(name);
            if skill_dir.exists() {
                std::fs::remove_dir_all(&skill_dir)
                    .map_err(|e| err(ErrorCode::InternalError, &format!("delete failed: {}", e)))?;
            }
            Ok(json!({ "ok": true }))
        }

        // === Plugins ===
        "plugins.list" => {
            let home = dirs::home_dir().unwrap_or_default();
            let search_dirs = vec![
                home.join(".loom").join("plugins"),
                home.join(".claude").join("plugins"),
            ];
            let mut plugins: Vec<Value> = Vec::new();
            for dir in &search_dirs {
                if !dir.exists() {
                    continue;
                }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !path.is_dir() {
                            continue;
                        }
                        // Every subdirectory under plugins/ counts as a plugin
                        let mut skill_count = count_skill_files(&path, 4);
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let mut version: Option<String> = None;
                        let mut description: Option<String> = None;
                        let mut mcp_server_count = 0u32;

                        // Try reading manifest.json
                        if let Ok(content) = std::fs::read_to_string(path.join("manifest.json")) {
                            if let Ok(manifest) = serde_json::from_str::<Value>(&content) {
                                version = manifest
                                    .get("version")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                description = manifest
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                skill_count = manifest
                                    .get("skills")
                                    .and_then(|v| v.as_array())
                                    .map(|a| a.len() as u32)
                                    .unwrap_or(skill_count);
                                mcp_server_count = manifest
                                    .get("mcp_servers")
                                    .and_then(|v| v.as_array())
                                    .map(|a| a.len() as u32)
                                    .unwrap_or(0);
                            }
                        }

                        plugins.push(json!({
                            "name": name,
                            "version": version,
                            "description": description,
                            "path": path.display().to_string(),
                            "skill_count": skill_count,
                            "mcp_server_count": mcp_server_count,
                        }));
                    }
                }
            }
            Ok(json!({ "plugins": plugins }))
        }

        "plugins.reload" => {
            // Re-scan plugins + standard skill dirs and update orchestrator
            let home = dirs::home_dir().unwrap_or_default();
            let data_dir = home.join(".loom");
            let mut loader = SkillLoader::new();
            loader.add_standard_paths(&data_dir);
            match loader.discover() {
                Ok(skills) => {
                    let ctx: String = skills
                        .iter()
                        .map(|s| format!("- {}", s.manifest.name))
                        .collect::<Vec<_>>()
                        .join("\n");
                    state.orchestrator.set_skill_context(ctx).await;
                    let bodies: std::collections::HashMap<String, String> = skills
                        .iter()
                        .map(|s| (s.manifest.name.clone(), s.body.clone()))
                        .collect();
                    state.orchestrator.set_skill_bodies(bodies).await;
                    tracing::info!("[plugins.reload] {} skills reloaded", skills.len());
                    Ok(json!({ "ok": true, "skill_count": skills.len() }))
                }
                Err(e) => Err(err(
                    ErrorCode::InternalError,
                    &format!("skill discovery failed: {}", e),
                )),
            }
        }

        // === Knowledge Graph ===
        "kg.search" => {
            let query = p.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
            let rows = state
                .orchestrator
                .kg_search(query, limit)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "rows": rows }))
        }
        "kg.stats" => {
            let stats = state
                .orchestrator
                .kg_stats()
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(serde_json::to_value(stats).unwrap_or_default())
        }
        "kg.neighbors" => {
            let node_name = p.get("node_name").and_then(|v| v.as_str()).unwrap_or("");
            if node_name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "node_name required"));
            }
            let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(30) as usize;
            let graph = state
                .orchestrator
                .kg_neighbors(node_name, limit)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(serde_json::to_value(graph).unwrap_or_default())
        }
        "kg.walk" => {
            let start_name = p.get("start_name").and_then(|v| v.as_str()).unwrap_or("");
            if start_name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "start_name required"));
            }
            let max_depth = p.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(2) as u8;
            let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            let graph = state
                .orchestrator
                .kg_walk(start_name, max_depth, limit)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(serde_json::to_value(graph).unwrap_or_default())
        }

        "kg.list" => {
            let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            let offset = p.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let scope = p.get("scope").and_then(|v| v.as_str());
            let nodes = state
                .orchestrator
                .kg_list_nodes(limit, offset, scope)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "nodes": nodes }))
        }

        "kg.node.delete" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            let deleted = state
                .orchestrator
                .kg_delete_node(name)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "deleted": deleted }))
        }

        "kg.edge.delete" => {
            let source = p.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let target = p.get("target").and_then(|v| v.as_str()).unwrap_or("");
            let relation = p.get("relation").and_then(|v| v.as_str()).unwrap_or("");
            if source.is_empty() || target.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "source and target required"));
            }
            let deleted = state
                .orchestrator
                .kg_delete_edge(source, target, relation)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "deleted": deleted }))
        }

        // === Cognition Records ===
        "cognitions.list" => {
            let subject = p.get("subject").and_then(|v| v.as_str()).unwrap_or("USER");
            let scope = p.get("scope").and_then(|v| v.as_str());
            let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            let offset = p.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let rows = state
                .orchestrator
                .cognition_list(subject, scope, limit, offset)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "rows": rows }))
        }
        "cognitions.snapshots" => {
            let cognition_id = p.get("cognition_id").and_then(|v| v.as_i64()).unwrap_or(0);
            if cognition_id == 0 {
                return Err(err(ErrorCode::InvalidRequest, "cognition_id required"));
            }
            let snapshots = state
                .orchestrator
                .cognition_snapshots(cognition_id)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "snapshots": snapshots }))
        }
        "cognitions.subjects" => {
            let subjects = state
                .orchestrator
                .cognition_list_subjects()
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "subjects": subjects }))
        }
        "kg.prune" => {
            let older_than_days = p.get("older_than_days").and_then(|v| v.as_i64()).unwrap_or(30);
            let pruned_count = state
                .orchestrator
                .kg_prune(older_than_days)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "pruned_count": pruned_count }))
        }

        // === Token Usage Stats ===
        "stats.token_summary" => {
            let from = p
                .get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("1970-01-01");
            let to = p.get("to").and_then(|v| v.as_str()).unwrap_or("2099-12-31");
            let summary = state
                .orchestrator
                .token_summary(from, to)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(summary)
        }

        "stats.token_history" => {
            let from = p
                .get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("1970-01-01");
            let to = p.get("to").and_then(|v| v.as_str()).unwrap_or("2099-12-31");
            let granularity = p
                .get("granularity")
                .and_then(|v| v.as_str())
                .unwrap_or("day");
            let history = state
                .orchestrator
                .token_history(from, to, granularity)
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(history)
        }

        // Fallback
        _ => Err(err(
            ErrorCode::MethodNotFound,
            &format!("method '{}' not found", req.method),
        )),
    }
}

/// Parse attached_files from frontend JSON-RPC params into ContentPart::Image items.
/// Handles both data URL thumbnails (pasted images) and file paths (picked files).
fn parse_attached_images(p: &Value) -> Vec<ContentPart> {
    let files = p
        .get("attached_files")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    tracing::info!(file_count = files.len(), "parse_attached_images: entry");

    let mut parts = Vec::new();
    for file in files {
        let mime_type = file
            .get("mime_type")
            .and_then(|v| v.as_str())
            .unwrap_or("image/png");

        let name = file.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let has_thumb = file
            .get("thumbnail")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let has_path = file
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        tracing::info!(%name, %mime_type, has_thumb, has_path, "parse_attached_images: processing file");

        if !mime_type.starts_with("image/") {
            tracing::debug!(%name, %mime_type, "skipped non-image file");
            continue;
        }

        let data = if let Some(thumb) = file.get("thumbnail").and_then(|v| v.as_str()) {
            if thumb.is_empty() {
                tracing::warn!(%name, "empty thumbnail");
                continue;
            }
            // data URL format: "data:image/png;base64,XXXX"
            if let Some(comma) = thumb.find(',') {
                thumb[comma + 1..].to_string()
            } else {
                thumb.to_string()
            }
        } else if let Some(ref path) = file.get("path").and_then(|v| v.as_str()) {
            if path.is_empty() {
                tracing::warn!(%name, "empty thumbnail and empty path, skipping image");
                continue;
            }
            match std::fs::read(path) {
                Ok(bytes) => base64::engine::general_purpose::STANDARD.encode(&bytes),
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "failed to read image file");
                    continue;
                }
            }
        } else {
            tracing::warn!(%name, %mime_type, has_thumb, has_path, "no thumbnail or path for image, skipping");
            continue;
        };

        if data.is_empty() {
            continue;
        }

        parts.push(ContentPart::Image {
            source_type: "base64".to_string(),
            media_type: mime_type.to_string(),
            data,
        });
    }

    parts
}

/// Text-based file extensions whose contents can be injected into the prompt.
fn is_text_extension(name: &str) -> bool {
    let lower = name.to_lowercase();
    let text_exts = [
        ".txt",
        ".md",
        ".rs",
        ".py",
        ".js",
        ".ts",
        ".tsx",
        ".jsx",
        ".go",
        ".java",
        ".c",
        ".cpp",
        ".h",
        ".hpp",
        ".cs",
        ".rb",
        ".php",
        ".swift",
        ".kt",
        ".scala",
        ".sh",
        ".bash",
        ".zsh",
        ".fish",
        ".ps1",
        ".bat",
        ".json",
        ".yaml",
        ".yml",
        ".toml",
        ".ini",
        ".cfg",
        ".conf",
        ".xml",
        ".html",
        ".css",
        ".scss",
        ".less",
        ".svelte",
        ".vue",
        ".sql",
        ".graphql",
        ".proto",
        ".env",
        ".lock",
        ".dockerfile",
        ".makefile",
        ".log",
        ".csv",
        ".tsv",
        ".r",
        ".m",
        ".lua",
        ".pl",
        ".ex",
        ".exs",
        ".erl",
        ".hrl",
        ".zig",
        ".nim",
        ".v",
        ".dart",
        ".jl",
    ];
    text_exts.iter().any(|ext| lower.ends_with(ext))
        || lower.contains("makefile")
        || lower.contains("dockerfile")
}

/// Read text-based non-image attached files and return formatted content for the prompt.
fn parse_attached_file_contents(p: &Value) -> String {
    let files = p
        .get("attached_files")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    let mut result = String::new();

    for file in files {
        let mime_type = file.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
        let name = file.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let path = file.get("path").and_then(|v| v.as_str()).unwrap_or("");

        // Images are handled by parse_attached_images separately
        if mime_type.starts_with("image/") {
            continue;
        }
        if path.is_empty() {
            continue;
        }

        // Size check: skip files larger than 500 KB
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                tracing::debug!(%name, %path, "attached file not found on disk");
                continue;
            }
        };
        if metadata.len() > 512_000 {
            tracing::debug!(%name, size = metadata.len(), "attached file too large, skipping");
            continue;
        }

        // Only read text-based files (by MIME type or extension)
        let is_text = mime_type.starts_with("text/")
            || mime_type == "application/json"
            || mime_type.contains("xml")
            || mime_type.contains("javascript")
            || mime_type.contains("typescript")
            || is_text_extension(name);

        if !is_text {
            tracing::debug!(%name, %mime_type, "skipping non-text attached file");
            continue;
        }

        match std::fs::read_to_string(path) {
            Ok(content) => {
                if content.is_empty() {
                    continue;
                }
                result.push_str(&format!(
                    "\n\n<attached_file name=\"{name}\">\n{content}\n</attached_file>",
                ));
                tracing::info!(%name, len = content.len(), "attached file content injected");
            }
            Err(e) => {
                tracing::debug!(%name, error = %e, "failed to read attached file");
            }
        }
    }

    result
}

fn count_skill_files(dir: &std::path::Path, max_depth: u32) -> u32 {
    if max_depth == 0 || !dir.is_dir() {
        return 0;
    }
    let mut count = 0u32;
    if dir.join("SKILL.md").exists() {
        count += 1;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_skill_files(&path, max_depth - 1);
            }
        }
    }
    count
}

fn err(code: ErrorCode, msg: &str) -> JsonRpcError {
    JsonRpcError {
        code,
        message: msg.to_string(),
        data: None,
    }
}
