//! JSON-RPC 2.0 dispatch — routes incoming requests to orchestrator methods.
//!
//! Supported: system, agent, chat, session, config, mcp, tools, skills.

use std::sync::Arc;
use std::collections::HashMap;

use loom_types::{ErrorCode, JsonRpcError, JsonRpcRequest, JsonRpcResponse};
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
        };
        self.sessions.write().await.insert(id.clone(), session.clone());
        session
    }

    pub async fn get(&self, id: &str) -> Option<SessionData> {
        self.sessions.read().await.get(id).cloned()
    }

    pub async fn get_or_create(&self, path: Option<&str>) -> SessionData {
        if let Some(id) = path {
            if let Some(s) = self.get(id).await { return s; }
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
        } else { false }
    }

    pub async fn delete(&self, id: &str) -> bool {
        self.sessions.write().await.remove(id).is_some()
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
            let result = state.orchestrator.process_message(content).await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("default");
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

        // === Session ===
        "session.list" => Ok(json!({ "sessions": state.sessions.list().await })),
        "session.create" => {
            let cwd = p.get("cwd").and_then(|v| v.as_str());
            let s = state.sessions.create(cwd).await;
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
            Ok(json!({ "ok": ok }))
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
