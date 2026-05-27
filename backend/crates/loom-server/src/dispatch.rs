//! JSON-RPC 2.0 dispatch — routes incoming requests to orchestrator methods.
//!
//! Supported: system, agent, chat, session, config, mcp, tools, skills.

use std::sync::Arc;
use std::collections::HashMap;

use loom_types::{AgentConfig, ErrorCode, JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use loom_types::Message as LoomMessage;
use serde_json::{json, Value};
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
        self.sessions.write().await.insert(id.clone(), session.clone());
        session
    }

    pub async fn get(&self, id: &str) -> Option<SessionData> {
        self.sessions.read().await.get(id).cloned()
    }

    pub async fn get_or_create(&self, path: Option<&str>) -> SessionData {
        if let Some(id) = path
            && let Some(s) = self.get(id).await { return s; }
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
        } else { false }
    }

    pub async fn delete(&self, id: &str) -> bool {
        self.sessions.write().await.remove(id).is_some()
    }

    /// Restore a persisted session on startup.
    pub async fn restore(&self, id: String, created_at: String, message_count: usize, title: Option<String>) {
        self.sessions.write().await.insert(id.clone(), SessionData {
            id, created_at, message_count, title, messages: Vec::new(), agent_config_name: None,
        });
    }

    pub async fn bind_agent(&self, session_id: &str, agent_config_name: &str) -> Result<(), String> {
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
        self.sessions.read().await.get(session_id).and_then(|s| s.agent_config_name.clone())
    }
}

pub async fn dispatch_handler(
    state: Arc<AppState>,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    let result = dispatch_method(&state, &req).await;
    let (result_val, error_val) = match result {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    };
    JsonRpcResponse { jsonrpc: "2.0".into(), result: result_val, error: error_val, id: req.id }
}

pub async fn dispatch_method(state: &AppState, req: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
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
            let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("default");

            // Resolve agent config for this session
            let config_name = state.sessions.get_bound_agent(session_id).await
                .unwrap_or_else(|| "default".to_string());
            let agent_config = state.orchestrator.agent_config_get(&config_name).await
                .unwrap_or_default();

            let result = state.orchestrator.process_message_with_config(content, session_id, &agent_config).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            state.sessions.add_message(session_id, "user", content).await;
            state.sessions.add_message(session_id, "assistant", &result.response).await;
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
            state.orchestrator.agent_status(&loom_types::AgentId::from(id)).await
                .map(|s| serde_json::to_value(s).unwrap_or_default())
                .map_err(|e| err(ErrorCode::AgentNotFound, &e.to_string()))
        }
        "agent.kill" => {
            let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
            state.orchestrator.kill_agent(&loom_types::AgentId::from(id)).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "agent.config.list" => {
            let configs = state.orchestrator.agent_config_list().await;
            Ok(json!({ "configs": configs }))
        }
        "agent.config.get" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("default");
            let config = state.orchestrator.agent_config_get(name).await
                .map_err(|e| err(ErrorCode::AgentNotFound, &e.to_string()))?;
            Ok(serde_json::to_value(config).unwrap_or_default())
        }
        "agent.config.create" => {
            let config: AgentConfig = serde_json::from_value(p.clone())
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            if config.name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            state.orchestrator.agent_config_create(config).await
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "agent.config.update" => {
            let config: AgentConfig = serde_json::from_value(p.clone())
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            if config.name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            state.orchestrator.agent_config_update(config).await
                .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }
        "agent.config.delete" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "name required"));
            }
            if name == "default" {
                return Err(err(ErrorCode::InvalidRequest, "cannot delete default config"));
            }
            state.orchestrator.agent_config_delete(name).await
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
            let id = p.get("session_id").or_else(|| p.get("path")).and_then(|v| v.as_str());
            let s = state.sessions.get_or_create(id).await;
            Ok(json!({ "session_id": s.id, "path": s.id, "title": s.title }))
        }
        "session.messages" => {
            let id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("default");
            let s = state.sessions.get_or_create(Some(id)).await;
            Ok(json!({ "messages": s.messages, "hasMore": false }))
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
            if ok { state.orchestrator.delete_session_persisted(id).await; }
            Ok(json!({ "ok": ok }))
        }
        "session.bind_agent" => {
            let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("default");
            let config_name = p.get("agent_config_name").and_then(|v| v.as_str()).unwrap_or("default");

            // Verify the config exists
            let _ = state.orchestrator.agent_config_get(config_name).await
                .map_err(|_e| err(ErrorCode::InvalidRequest, &format!("agent config '{}' not found", config_name)))?;

            state.sessions.bind_agent(session_id, config_name).await
                .map_err(|e| err(ErrorCode::InternalError, &e))?;
            Ok(json!({ "ok": true }))
        }

        // === Config ===
        "config.get" => {
            let key = p.get("key").and_then(|v| v.as_str());
            Ok(json!({ "config": { "key": key } }))
        }
        "config.set" => {
            Ok(json!({ "ok": true }))
        }

        // === Model ===
        "model.list" => Ok(json!({ "models": [], "activeModel": null })),
        "model.switch" => Ok(json!({ "ok": true })),

        // === MCP ===
        "mcp.list_servers" => {
            let names = state.orchestrator.mcp_client().server_names().await;
            Ok(json!({ "servers": names }))
        }
        "mcp.list_tools" => {
            let defs = state.orchestrator.mcp_client().all_tool_definitions().await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "tools": defs }))
        }
        "mcp.list_resources" => {
            let server = p.get("server").and_then(|v| v.as_str()).unwrap_or("");
            if server.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "server required"));
            }
            let resources = state.orchestrator.mcp_client().list_resources(server).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "resources": resources }))
        }
        "mcp.read_resource" => {
            let server = p.get("server").and_then(|v| v.as_str()).unwrap_or("");
            let uri = p.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            if server.is_empty() || uri.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "server and uri required"));
            }
            let contents = state.orchestrator.mcp_client().read_resource(server, uri).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(serde_json::to_value(contents).unwrap_or_default())
        }
        "mcp.list_resource_templates" => {
            let server = p.get("server").and_then(|v| v.as_str()).unwrap_or("");
            if server.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "server required"));
            }
            let templates = state.orchestrator.mcp_client().list_resource_templates(server).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "templates": templates }))
        }
        "mcp.list_prompts" => {
            let server = p.get("server").and_then(|v| v.as_str()).unwrap_or("");
            if server.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "server required"));
            }
            let prompts = state.orchestrator.mcp_client().list_prompts(server).await
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
            let result = state.orchestrator.mcp_client().get_prompt(server, name, args).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(serde_json::to_value(result).unwrap_or_default())
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
            let result = state.orchestrator.lsp_client().diagnostics(file_path).await
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
            let result = state.orchestrator.lsp_client().completion(file_path, line, character).await
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
            let result = state.orchestrator.lsp_client().hover(file_path, line, character).await
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
            let result = state.orchestrator.lsp_client().definition(file_path, line, character).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(result)
        }
        "lsp.references" => {
            let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            let line = p.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let character = p.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let include_decl = p.get("include_declaration").and_then(|v| v.as_bool()).unwrap_or(true);
            if file_path.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "file_path required"));
            }
            let result = state.orchestrator.lsp_client().references(file_path, line, character, include_decl).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(result)
        }
        "lsp.symbols" => {
            let file_path = p.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            if file_path.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "file_path required"));
            }
            let result = state.orchestrator.lsp_client().document_symbols(file_path).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(result)
        }
        "lsp.shutdown" => {
            let language = p.get("language").and_then(|v| v.as_str()).unwrap_or("");
            if language.is_empty() {
                return Err(err(ErrorCode::InvalidRequest, "language required (e.g. 'rust', 'typescript')"));
            }
            state.orchestrator.lsp_client().shutdown(language).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            Ok(json!({ "ok": true }))
        }

        // === Tools ===
        "tools.list" => {
            let names = state.orchestrator.tool_registry().await.list_names();
            Ok(json!({ "tools": names }))
        }

        // === Skills ===
        "skills.list" => Ok(json!({ "skills": [] })),

        // Fallback
        _ => Err(err(ErrorCode::MethodNotFound, &format!("method '{}' not found", req.method))),
    }
}

fn err(code: ErrorCode, msg: &str) -> JsonRpcError {
    JsonRpcError { code, message: msg.to_string(), data: None }
}
