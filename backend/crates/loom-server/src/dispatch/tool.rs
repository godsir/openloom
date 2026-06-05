//! Tool dispatch handlers — tool.respond (permission approval)

use loom_types::JsonRpcError;
use serde_json::{Value, json};

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "tool.respond" => Some(handle_tool_respond(state, p).await),
        _ => None,
    }
}

async fn handle_tool_respond(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let call_id = p
        .get("call_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(loom_types::ErrorCode::InvalidRequest, "call_id required"))?;
    let approved = p.get("approved").and_then(|v| v.as_bool()).unwrap_or(false);

    let mut pending = state.orchestrator.pending_permissions().await;
    if let Some(tx) = pending.remove(call_id) {
        let _ = tx.send(approved);
        tracing::info!(call_id = %call_id, approved, "tool.respond: permission response sent");
        Ok(json!({ "ok": true, "call_id": call_id, "approved": approved }))
    } else {
        tracing::warn!(call_id = %call_id, "tool.respond: no pending permission request for call_id");
        Err(err(
            loom_types::ErrorCode::InvalidRequest,
            &format!("no pending permission request for call_id: {}", call_id),
        ))
    }
}
