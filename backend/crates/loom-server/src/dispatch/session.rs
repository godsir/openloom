//! Session dispatch handlers — session.* / workspace.* + SessionStore

use std::collections::HashMap;

use loom_types::Message as LoomMessage;
use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};
use tokio::sync::RwLock;

use super::err;
use crate::AppState;

// ---------------------------------------------------------------------------
// SessionStore (moved from dispatch.rs)
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct SessionStore {
    sessions: RwLock<HashMap<String, SessionData>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionData {
    pub id: String,
    pub created_at: String,
    #[serde(default = "default_updated_at")]
    pub updated_at: String,
    pub message_count: usize,
    pub title: Option<String>,
    pub messages: Vec<LoomMessage>,
    #[serde(default)]
    pub agent_config_name: Option<String>,
}

fn default_updated_at() -> String {
    chrono::Utc::now().to_rfc3339()
}

impl SessionStore {
    pub async fn list(&self) -> Vec<SessionData> {
        let mut sessions: Vec<SessionData> = self.sessions.read().await.values().cloned().collect();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }

    pub async fn create(&self, cwd: Option<&str>) -> SessionData {
        let id = uuid::Uuid::now_v7().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let session = SessionData {
            id: id.clone(),
            created_at: now.clone(),
            updated_at: now,
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
            s.updated_at = chrono::Utc::now().to_rfc3339();
        }
    }

    pub async fn rename(&self, id: &str, title: &str) -> bool {
        if let Some(s) = self.sessions.write().await.get_mut(id) {
            s.title = Some(title.to_string());
            s.updated_at = chrono::Utc::now().to_rfc3339();
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
        updated_at: String,
        message_count: usize,
        title: Option<String>,
    ) {
        self.sessions.write().await.insert(
            id.clone(),
            SessionData {
                id,
                created_at,
                updated_at,
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

// ---------------------------------------------------------------------------
// Session & workspace handler
// ---------------------------------------------------------------------------

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        // Session
        "session.list" => Some(handle_session_list(state).await),
        "session.create" => Some(handle_session_create(state, p).await),
        "session.switch" => Some(handle_session_switch(state, p).await),
        "session.messages" => Some(handle_session_messages(state, p).await),
        "session.delete_message" => Some(handle_session_delete_message(state, p).await),
        "session.rename" => Some(handle_session_rename(state, p).await),
        "session.auto_title" => Some(handle_session_auto_title(state, p).await),
        "session.delete" => Some(handle_session_delete(state, p).await),
        "session.bind_agent" => Some(handle_session_bind_agent(state, p).await),
        // Workspace
        "workspace.get" => Some(handle_workspace_get(state, p).await),
        "workspace.set_session" => Some(handle_workspace_set_session(state, p).await),
        "workspace.set_default" => Some(handle_workspace_set_default(state, p).await),
        _ => None,
    }
}

// --- session.list ---

async fn handle_session_list(state: &AppState) -> Result<Value, JsonRpcError> {
    Ok(json!({ "sessions": state.sessions.list().await }))
}

// --- session.create ---

async fn handle_session_create(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let cwd = p.get("cwd").and_then(|v| v.as_str());
    let s = state.sessions.create(cwd).await;
    state.orchestrator.ensure_session_persisted(&s.id).await;
    Ok(json!({ "session_id": s.id, "path": s.id, "created_at": s.created_at }))
}

// --- session.switch ---

async fn handle_session_switch(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p
        .get("session_id")
        .or_else(|| p.get("path"))
        .and_then(|v| v.as_str());
    let s = state.sessions.get_or_create(id).await;
    Ok(json!({ "session_id": s.id, "path": s.id, "title": s.title }))
}

// --- session.messages ---

async fn handle_session_messages(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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

// --- session.delete_message ---

async fn handle_session_delete_message(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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

// --- session.rename ---

async fn handle_session_rename(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let title = p.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let ok = state.sessions.rename(id, title).await;
    if ok {
        state.orchestrator.rename_session_persisted(id, title).await;
    }
    Ok(json!({ "ok": ok }))
}

// --- session.auto_title ---

async fn handle_session_auto_title(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    if id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "session_id required"));
    }
    match state.orchestrator.auto_title(id).await {
        Ok(title) => {
            let _ = state.sessions.rename(id, &title).await;
            state
                .orchestrator
                .rename_session_persisted(id, &title)
                .await;
            Ok(json!({ "title": title }))
        }
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

// --- session.delete ---

async fn handle_session_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let ok = state.sessions.delete(id).await;
    if ok {
        state.orchestrator.delete_session_persisted(id).await;
    }
    Ok(json!({ "ok": ok }))
}

// --- session.bind_agent ---

async fn handle_session_bind_agent(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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
    // Persist the binding so it survives restarts
    state
        .orchestrator
        .bind_agent_persisted(session_id, config_name)
        .await;
    Ok(json!({ "ok": true }))
}

// --- workspace.get ---

async fn handle_workspace_get(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let session_id = p.get("session_id").and_then(|v| v.as_str());

    let workspace = if let Some(sid) = session_id {
        let session_ws = state
            .orchestrator
            .get_session_workspace(sid)
            .await
            .ok()
            .flatten();
        if session_ws.is_some() {
            session_ws
        } else {
            state
                .orchestrator
                .get_default_workspace()
                .await
                .ok()
                .flatten()
        }
    } else {
        state
            .orchestrator
            .get_default_workspace()
            .await
            .ok()
            .flatten()
    };

    Ok(json!({ "workspace": workspace }))
}

// --- workspace.set_session ---

async fn handle_workspace_set_session(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let session_id = p
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "session_id required"))?;
    let path = p
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "path required"))?;

    state
        .orchestrator
        .set_session_workspace(session_id, path)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- workspace.set_default ---

async fn handle_workspace_set_default(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let path = p
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "path required"))?;

    state
        .orchestrator
        .set_default_workspace(path)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}
