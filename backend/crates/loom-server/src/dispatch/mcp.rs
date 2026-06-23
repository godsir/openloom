//! MCP dispatch handlers — mcp.*

use std::collections::HashSet;

use loom_mcp::McpServerConfig;
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
        "mcp.list_servers" => Some(handle_mcp_list_servers(state).await),
        "mcp.list_tools" => Some(handle_mcp_list_tools(state).await),
        "mcp.list_resources" => Some(handle_mcp_list_resources(state, p).await),
        "mcp.read_resource" => Some(handle_mcp_read_resource(state, p).await),
        "mcp.list_resource_templates" => Some(handle_mcp_list_resource_templates(state, p).await),
        "mcp.list_prompts" => Some(handle_mcp_list_prompts(state, p).await),
        "mcp.get_prompt" => Some(handle_mcp_get_prompt(state, p).await),
        "mcp.connect" => Some(handle_mcp_connect(state, p).await),
        "mcp.disconnect" => Some(handle_mcp_disconnect(state, p).await),
        "mcp.server_health" => Some(handle_mcp_server_health(state, p).await),
        "mcp.config.list" => Some(handle_mcp_config_list(state).await),
        "mcp.config.save" => Some(handle_mcp_config_save(state, p).await),
        "mcp.config.delete" => Some(handle_mcp_config_delete(state, p).await),
        _ => None,
    }
}

// --- mcp.list_servers ---

async fn handle_mcp_list_servers(state: &AppState) -> Result<Value, JsonRpcError> {
    let names = state.orchestrator.mcp_client().server_names().await;
    Ok(json!({ "servers": names }))
}

// --- mcp.list_tools ---

async fn handle_mcp_list_tools(state: &AppState) -> Result<Value, JsonRpcError> {
    let defs = state
        .orchestrator
        .mcp_client()
        .all_tool_definitions()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "tools": defs }))
}

// --- mcp.list_resources ---

async fn handle_mcp_list_resources(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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

// --- mcp.read_resource ---

async fn handle_mcp_read_resource(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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

// --- mcp.list_resource_templates ---

async fn handle_mcp_list_resource_templates(
    state: &AppState,
    p: &Value,
) -> Result<Value, JsonRpcError> {
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

// --- mcp.list_prompts ---

async fn handle_mcp_list_prompts(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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

// --- mcp.get_prompt ---

async fn handle_mcp_get_prompt(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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

// --- mcp.connect ---

async fn handle_mcp_connect(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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
    if persist && let Err(e) = state.orchestrator.save_mcp_server(&config, autostart).await {
        tracing::warn!(error = %e, "failed to persist MCP server config");
    }
    state
        .orchestrator
        .connect_mcp_server(config)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- mcp.disconnect ---

async fn handle_mcp_disconnect(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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
    // Mark autostart=false so the server stays disconnected across restarts.
    // Re-connecting via the UI (mcp.connect with persist=true) sets it back to true.
    if let Ok(saved) = state.orchestrator.list_saved_mcp_servers().await
        && let Some((cfg, _)) = saved.iter().find(|(c, _)| c.name == name)
    {
        let cfg = cfg.clone();
        let _ = state.orchestrator.save_mcp_server(&cfg, false).await;
    }
    Ok(json!({ "ok": true }))
}

// --- mcp.server_health ---

async fn handle_mcp_server_health(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    let healthy = state.orchestrator.mcp_client().server_health(name).await;
    Ok(json!({ "healthy": healthy }))
}

// --- mcp.config.list ---

async fn handle_mcp_config_list(state: &AppState) -> Result<Value, JsonRpcError> {
    let configs = state
        .orchestrator
        .list_saved_mcp_servers()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    let live: HashSet<String> = state
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

// --- mcp.config.save ---

async fn handle_mcp_config_save(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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

// --- mcp.config.delete ---

async fn handle_mcp_config_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
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
