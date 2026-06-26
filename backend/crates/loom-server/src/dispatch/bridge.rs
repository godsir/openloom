//! Bridge dispatch handlers — bridge.*

use loom_bridge::{InstanceConfig, Platform};
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
        "bridge.list_configs" => Some(handle_bridge_list_configs(state).await),
        "bridge.set_config" => Some(handle_bridge_set_config(state, p).await),
        "bridge.delete_config" => Some(handle_bridge_delete_config(state, p).await),
        "bridge.start_channel" => Some(handle_bridge_start_channel(state, p).await),
        "bridge.stop_channel" => Some(handle_bridge_stop_channel(state, p).await),
        "bridge.start_all" => Some(handle_bridge_start_all(state).await),
        "bridge.stop_all" => Some(handle_bridge_stop_all(state).await),
        "bridge.get_status" => Some(handle_bridge_get_status(state, p).await),
        "bridge.test_connectivity" => Some(handle_bridge_test_connectivity(state, p).await),
        _ => None,
    }
}

// --- bridge.list_configs ---

async fn handle_bridge_list_configs(state: &AppState) -> Result<Value, JsonRpcError> {
    let configs = state.bridge_manager.list_configs().await;
    let items: Vec<Value> = configs
        .iter()
        .map(|c| instance_config_to_json(c))
        .collect();
    Ok(json!({ "configs": items }))
}

// --- bridge.set_config ---

async fn handle_bridge_set_config(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let platform = parse_platform(p)?;
    let instance_id = p
        .get("instance_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let config = p
        .get("config")
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "config required"))?;

    let cfg = json_to_instance_config(platform, instance_id, config)?;
    state.bridge_manager.upsert_config(cfg).await;
    Ok(json!({ "ok": true }))
}

// --- bridge.delete_config ---

async fn handle_bridge_delete_config(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let platform = parse_platform(p)?;
    let instance_id = p
        .get("instance_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    state
        .bridge_manager
        .remove_config(platform, instance_id)
        .await;
    Ok(json!({ "ok": true }))
}

// --- bridge.start_channel ---

async fn handle_bridge_start_channel(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let platform = parse_platform(p)?;
    let instance_id = p
        .get("instance_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    state
        .bridge_manager
        .start_instance(platform, instance_id)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- bridge.stop_channel ---

async fn handle_bridge_stop_channel(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let platform = parse_platform(p)?;
    let instance_id = p
        .get("instance_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    state
        .bridge_manager
        .stop_instance(platform, instance_id)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- bridge.start_all ---

async fn handle_bridge_start_all(state: &AppState) -> Result<Value, JsonRpcError> {
    state.bridge_manager.start_all_enabled().await;
    Ok(json!({ "ok": true }))
}

// --- bridge.stop_all ---

async fn handle_bridge_stop_all(state: &AppState) -> Result<Value, JsonRpcError> {
    state.bridge_manager.shutdown_all().await;
    Ok(json!({ "ok": true }))
}

// --- bridge.get_status ---

async fn handle_bridge_get_status(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    // Optional platform filter
    let platform_filter = p
        .get("platform")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<Platform>(&format!("\"{}\"", s)).ok());

    let status_map = match platform_filter {
        Some(plat) => {
            let healths = state.bridge_manager.platform_health(plat).await;
            let items: Vec<Value> = healths
                .iter()
                .map(|(id, h)| {
                    json!({
                        "instance_id": id,
                        "health": format!("{:?}", h),
                    })
                })
                .collect();
            json!({
                "platform": plat.name(),
                "instances": items,
            })
        }
        None => {
            let all = state.bridge_manager.health_status().await;
            let items: Vec<Value> = all
                .iter()
                .map(|(key, health)| {
                    json!({
                        "key": key,
                        "health": format!("{:?}", health),
                    })
                })
                .collect();
            json!({ "instances": items })
        }
    };

    Ok(status_map)
}

// --- bridge.test_connectivity ---

async fn handle_bridge_test_connectivity(
    state: &AppState,
    p: &Value,
) -> Result<Value, JsonRpcError> {
    let platform = parse_platform(p)?;
    let instance_id = p
        .get("instance_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    match state
        .bridge_manager
        .validate_credentials(platform, instance_id)
        .await
    {
        Ok(()) => Ok(json!({
            "success": true,
            "message": format!("connectivity to {}:{} ok", platform.name(), instance_id),
        })),
        Err(e) => Ok(json!({
            "success": false,
            "message": format!("connectivity test failed: {}", e),
        })),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_platform(p: &Value) -> Result<Platform, JsonRpcError> {
    let s = p
        .get("platform")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "platform required"))?;
    serde_json::from_str::<Platform>(&format!("\"{}\"", s))
        .map_err(|e| err(ErrorCode::InvalidRequest, &format!("invalid platform: {}", e)))
}

fn instance_config_to_json(c: &InstanceConfig) -> Value {
    json!({
        "id": c.id,
        "platform": c.platform.name(),
        "instance_id": c.instance_id,
        "instance_name": c.instance_name,
        "enabled": c.enabled,
        "config_json": c.config_json,
        "dm_policy": format!("{}", c.dm_policy),
        "allow_from": c.allow_from,
        "group_policy": format!("{}", c.group_policy),
        "group_allow_from": c.group_allow_from,
        "agent_id": c.agent_id,
        "created_at": c.created_at,
        "updated_at": c.updated_at,
    })
}

fn json_to_instance_config(
    platform: Platform,
    instance_id: &str,
    config: &Value,
) -> Result<InstanceConfig, JsonRpcError> {
    let now = chrono::Utc::now().timestamp_millis();
    // Try to carry forward existing id/timestamps if present, else generate new.
    let id = config
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let created_at = config
        .get("created_at")
        .and_then(|v| v.as_i64())
        .unwrap_or(now);
    let enabled = config
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let dm_policy: loom_bridge::AccessMode = config
        .get("dm_policy")
        .and_then(|v| v.as_str())
        .and_then(|s| match s {
            "open" => Some(loom_bridge::AccessMode::Open),
            "pairing" => Some(loom_bridge::AccessMode::Pairing),
            "allowlist" => Some(loom_bridge::AccessMode::Allowlist),
            "disabled" => Some(loom_bridge::AccessMode::Disabled),
            _ => None,
        })
        .unwrap_or(loom_bridge::AccessMode::Pairing);

    let group_policy: loom_bridge::AccessMode = config
        .get("group_policy")
        .and_then(|v| v.as_str())
        .and_then(|s| match s {
            "open" => Some(loom_bridge::AccessMode::Open),
            "pairing" => Some(loom_bridge::AccessMode::Pairing),
            "allowlist" => Some(loom_bridge::AccessMode::Allowlist),
            "disabled" => Some(loom_bridge::AccessMode::Disabled),
            _ => None,
        })
        .unwrap_or(loom_bridge::AccessMode::Pairing);

    Ok(InstanceConfig {
        id,
        platform,
        instance_id: instance_id.to_string(),
        instance_name: config
            .get("instance_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{} {}", platform.name(), instance_id)),
        enabled,
        config_json: config
            .get("config_json")
            .cloned()
            .unwrap_or(serde_json::Value::Object(Default::default())),
        dm_policy,
        allow_from: config
            .get("allow_from")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        group_policy,
        group_allow_from: config
            .get("group_allow_from")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        agent_id: config
            .get("agent_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        created_at,
        updated_at: now,
    })
}
