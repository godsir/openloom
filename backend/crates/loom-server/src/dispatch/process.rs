//! Process dispatch handlers — process.spawn / process.kill / process.stdin / process.list

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
        "process.spawn" => Some(handle_spawn(state, p).await),
        "process.kill" => Some(handle_kill(state, p).await),
        "process.stdin" => Some(handle_stdin(state, p).await),
        "process.list" => Some(handle_list(state, p).await),
        _ => None,
    }
}

async fn handle_spawn(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let command = p.get("command").and_then(|v| v.as_str()).unwrap_or("");
    if command.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "command required"));
    }
    let cwd = p.get("cwd").and_then(|v| v.as_str());
    let name = p.get("name").and_then(|v| v.as_str());
    let env: Option<std::collections::HashMap<String, String>> = p
        .get("env")
        .and_then(|v| v.as_object())
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        });

    let pm = state.orchestrator.process_manager();
    match pm.spawn(command, cwd, env.as_ref(), name, "").await {
        Ok((pid, name)) => Ok(json!({ "pid": pid, "name": name })),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_kill(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let pid = p.get("pid").and_then(|v| v.as_str()).unwrap_or("");
    if pid.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "pid required"));
    }
    let pm = state.orchestrator.process_manager();
    match pm.kill(pid).await {
        Ok(true) => Ok(json!({ "killed": true })),
        Ok(false) => Ok(json!({ "killed": false })),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_stdin(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let pid = p.get("pid").and_then(|v| v.as_str()).unwrap_or("");
    let input = p.get("input").and_then(|v| v.as_str()).unwrap_or("");
    if pid.is_empty() || input.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "pid and input required"));
    }
    let pm = state.orchestrator.process_manager();
    match pm.stdin_write(pid, input).await {
        Ok(true) => Ok(json!({ "ok": true })),
        Ok(false) => Err(err(ErrorCode::InternalError, "process not found or exited")),
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

async fn handle_list(state: &AppState, _p: &Value) -> Result<Value, JsonRpcError> {
    let pm = state.orchestrator.process_manager();
    let procs = pm.list().await;
    Ok(serde_json::to_value(procs).unwrap_or(json!([])))
}
