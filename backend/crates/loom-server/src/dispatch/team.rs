//! Team dispatch handlers — team.config.*

use loom_types::config::team::TeamConfig;
use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};
use uuid::Uuid;

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "team.config.list" => Some(handle_team_config_list(state).await),
        "team.config.get" => Some(handle_team_config_get(state, p).await),
        "team.config.create" => Some(handle_team_config_create(state, p).await),
        "team.config.update" => Some(handle_team_config_update(state, p).await),
        "team.config.delete" => Some(handle_team_config_delete(state, p).await),
        "team.config.generate_members" => Some(handle_team_generate_members(state, p).await),
        _ => None,
    }
}

async fn handle_team_config_list(state: &AppState) -> Result<Value, JsonRpcError> {
    let configs = state.orchestrator.team_config_list().await;
    Ok(json!({ "teams": configs }))
}

async fn handle_team_config_get(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "id required"));
    }
    let config = state.orchestrator.team_config_get(id).await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(serde_json::to_value(config).unwrap_or_default())
}

async fn handle_team_config_create(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let mut config: TeamConfig = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    if config.name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    if config.id.is_empty() {
        config.id = Uuid::now_v7().to_string();
    }
    state.orchestrator.team_config_create(config.clone()).await
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    Ok(json!({ "id": config.id }))
}

async fn handle_team_config_update(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let config: TeamConfig = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    if config.id.is_empty() || config.name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "id and name required"));
    }
    state.orchestrator.team_config_update(config).await
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

async fn handle_team_config_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "id required"));
    }
    state.orchestrator.team_config_delete(id).await
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

async fn handle_team_generate_members(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let description = p.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let strategy = p.get("strategy").and_then(|v| v.as_str()).unwrap_or("synthesize");
    let captain_model = p.get("captain_model").and_then(|v| v.as_str());

    if name.trim().is_empty() && description.trim().is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name or description required"));
    }

    let members = state
        .orchestrator
        .team_members_generate(name.trim(), description.trim(), strategy, captain_model)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

    Ok(serde_json::to_value(members).unwrap_or(json!([])))
}
