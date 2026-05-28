//! JSON-RPC 2.0 dispatch — routes incoming requests to orchestrator methods.
//!
//! Supported: system, agent, chat, session, config, mcp, lsp, tools, skills, plugins.

use std::collections::HashMap;
use std::sync::Arc;

use loom_types::Message as LoomMessage;
use loom_types::{
    AgentConfig, ErrorCode, JsonRpcError, JsonRpcRequest, JsonRpcResponse, ModelConfig,
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
            "status": "ok", "version": "0.2.0",
            "agent_count": state.orchestrator.list_agents().await.len(),
            "tool_count": state.orchestrator.tool_registry().await.len(),
        })),

        // === Chat ===
        "chat.send" => {
            let content = p.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if content.is_empty() {
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
                .process_message_with_config(content, session_id, &agent_config, thinking_budget)
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
            tracing::info!(
                session_id = %id,
                in_memory_msgs = s.messages.len(),
                "session.messages: in-memory snapshot"
            );
            // If in-memory messages are empty, try loading from persisted history
            let msgs = if s.messages.is_empty() {
                match state.orchestrator.load_history(id).await {
                    Ok(_) => tracing::info!(session_id = %id, "load_history ok"),
                    Err(e) => tracing::warn!(session_id = %id, error = %e, "load_history failed"),
                }
                let history = state.orchestrator.session_history(id).await;
                tracing::info!(
                    session_id = %id,
                    history_len = history.len(),
                    "session.messages: loaded from DB"
                );
                if history.is_empty() {
                    s.messages
                } else {
                    history
                }
            } else {
                s.messages
            };
            tracing::info!(
                session_id = %id,
                returning = msgs.len(),
                "session.messages: returning to client"
            );
            Ok(json!({ "messages": msgs, "hasMore": false }))
        }
        "session.rename" => {
            let id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
            let title = p.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let ok = state.sessions.rename(id, title).await;
            Ok(json!({ "ok": ok }))
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
                        "base_url": c.base_url,
                        "is_active": active.as_deref() == Some(&c.name),
                        "capabilities": c.capabilities,
                        "api_format": c.api_format,
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
            let config: ModelConfig = serde_json::from_value(p.clone())
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            if config.name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            state
                .orchestrator
                .model_config_update(config)
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
            if backend.is_empty() || api_key.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "backend and api_key required"));
            }
            let env_name = format!("{}_API_KEY", backend.to_uppercase().replace('-', "_"));
            // Write to .loom/env file for persistence
            let home = dirs::home_dir().unwrap_or_default().join(".loom");
            let _ = std::fs::create_dir_all(&home);
            let env_file = home.join("env");
            let mut lines: Vec<String> = std::fs::read_to_string(&env_file)
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.starts_with(&format!("{}=", env_name)))
                .map(|l| l.to_string())
                .collect();
            lines.push(format!("{}={}", env_name, api_key));
            let _ = std::fs::write(&env_file, lines.join("\n"));
            // Also set in current process environment
            // SAFETY: single-threaded dispatch context, no concurrent env reads during write
            unsafe { std::env::set_var(&env_name, api_key); }
            Ok(json!({ "ok": true, "env_name": env_name }))
        }

        "model.discover" => {
            let backend = p.get("backend").and_then(|v| v.as_str()).unwrap_or("");
            let base_url = p.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
            let api_format = p.get("api_format").and_then(|v| v.as_str()).unwrap_or("openai");
            if base_url.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "base_url required"));
            }
            let env_name = format!("{}_API_KEY", backend.to_uppercase().replace('-', "_"));
            let api_key = std::env::var(&env_name).unwrap_or_default();
            let client = reqwest::Client::new();

            let models: Vec<String> = if api_format == "anthropic" {
                let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
                let resp = client
                    .get(&url)
                    .header("x-api-key", &api_key)
                    .header("anthropic-version", "2023-06-01")
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                    .map_err(|e| err(ErrorCode::InternalError, &format!("HTTP error: {}", e)))?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(err(ErrorCode::InternalError, &format!("API returned {}: {}", status, body)));
                }
                let body: Value = resp.json().await
                    .map_err(|e| err(ErrorCode::InternalError, &format!("Parse error: {}", e)))?;
                body.get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| arr.iter().filter_map(|item| item.get("id").and_then(|id| id.as_str()).map(|s| s.to_string())).collect())
                    .unwrap_or_default()
            } else {
                let url = format!("{}/models", base_url.trim_end_matches('/'));
                let resp = client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                    .map_err(|e| err(ErrorCode::InternalError, &format!("HTTP error: {}", e)))?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(err(ErrorCode::InternalError, &format!("API returned {}: {}", status, body)));
                }
                let body: Value = resp.json().await
                    .map_err(|e| err(ErrorCode::InternalError, &format!("Parse error: {}", e)))?;
                body.get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| arr.iter().filter_map(|item| item.get("id").and_then(|id| id.as_str()).map(|s| s.to_string())).collect())
                    .unwrap_or_default()
            };

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
            let model = p.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
            let home = dirs::home_dir().unwrap_or_default().join(".loom");
            let _ = std::fs::create_dir_all(&home);
            let config = json!({ "enabled": enabled, "model": model });
            let config_file = home.join("vision.json");
            std::fs::write(&config_file, serde_json::to_string_pretty(&config).unwrap_or_default())
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
                enabled_tools: p
                    .get("enabled_tools")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    }),
                disabled_tools: p
                    .get("disabled_tools")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    }),
            };
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
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            if language.is_empty() || command.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "language and command required"));
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
            match state.orchestrator.get_skill_body(name).await {
                Some(content) => Ok(json!({ "content": content })),
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
            Ok(json!({ "ok": true, "path": skill_dir.display().to_string(), "files_written": wrote }))
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
                        // Look for plugin.toml, manifest.json, or SKILL.md
                        let has_manifest = path.join("plugin.toml").exists()
                            || path.join("manifest.json").exists()
                            || path.join("SKILL.md").exists();
                        if !has_manifest {
                            continue;
                        }
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let mut version: Option<String> = None;
                        let mut description: Option<String> = None;
                        let mut skill_count = 0u32;
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
                                    .unwrap_or(0);
                                mcp_server_count = manifest
                                    .get("mcp_servers")
                                    .and_then(|v| v.as_array())
                                    .map(|a| a.len() as u32)
                                    .unwrap_or(0);
                            }
                        }
                        // Fallback: count SKILL.md files in subdirectories
                        if skill_count == 0 && path.join("SKILL.md").exists() {
                            skill_count = 1;
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

        // Fallback
        _ => Err(err(
            ErrorCode::MethodNotFound,
            &format!("method '{}' not found", req.method),
        )),
    }
}

fn err(code: ErrorCode, msg: &str) -> JsonRpcError {
    JsonRpcError {
        code,
        message: msg.to_string(),
        data: None,
    }
}
