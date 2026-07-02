//! Monitor dispatch handlers — monitor.list / monitor.kill

use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "monitor.list" => Some(handle_list(state, p).await),
        "monitor.kill" => Some(handle_kill(state, p).await),
        _ => None,
    }
}

async fn handle_list(state: &AppState, _p: &Value) -> Result<Value, JsonRpcError> {
    let mm = state.orchestrator.monitor_manager();
    let monitors = mm.list().await;
    Ok(serde_json::to_value(monitors).unwrap_or(json!([])))
}

async fn handle_kill(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let monitor_id = p
        .get("monitor_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if monitor_id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "monitor_id required"));
    }
    let mm = state.orchestrator.monitor_manager();
    match mm.kill(monitor_id).await {
        Ok(true) => Ok(json!({ "killed": true })),
        Ok(false) => Ok(json!({ "killed": false })),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}
