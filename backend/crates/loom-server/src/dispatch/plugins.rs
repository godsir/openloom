//! Plugins dispatch handlers — plugins.*

use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};
use std::collections::HashMap;

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    _p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "plugins.list" => Some(handle_plugins_list(state).await),
        "plugins.reload" => Some(handle_plugins_reload(state).await),
        _ => None,
    }
}

// --- plugins.list ---

async fn handle_plugins_list(state: &AppState) -> Result<Value, JsonRpcError> {
    let _ = state; // uses direct filesystem + PluginManager
    let home = dirs::home_dir().unwrap_or_default();
    let mut plugin_manager = loom_plugins::PluginManager::new();
    let _ = tokio::task::spawn_blocking(move || plugin_manager.discover(&home))
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

    let mut plugins: Vec<Value> = Vec::new();
    for plugin in plugin_manager.discovered() {
        let skill_count = plugin
            .manifest
            .skills
            .as_ref()
            .map(|s| s.paths.len())
            .unwrap_or(0) as u64;
        let mcp_server_count = plugin
            .manifest
            .mcp_servers
            .as_ref()
            .map(|s| s.len())
            .unwrap_or(0) as u64;
        let hook_config =
            loom_plugins::hooks::HookConfig::from_plugin_dir(&plugin.path).unwrap_or_default();
        let hook_count = hook_config.hooks.len() as u64;
        let has_settings = !plugin.manifest.settings.is_null();

        // Per-skill details: {name, path}
        let skills: Vec<Value> = plugin
            .manifest
            .skills
            .as_ref()
            .map(|s| {
                s.paths
                    .iter()
                    .map(|rel_path| {
                        let full_path = plugin.path.join(rel_path);
                        let name = if rel_path == "." {
                            plugin.manifest.name.clone()
                        } else {
                            rel_path.rsplit('/').next().unwrap_or(rel_path).to_string()
                        };
                        json!({
                            "name": name,
                            "path": full_path.display().to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Per-MCP-server details: {name, transport}
        let mcp_servers: Vec<Value> = plugin
            .manifest
            .mcp_servers
            .as_ref()
            .map(|servers| {
                servers
                    .iter()
                    .map(|s| {
                        json!({
                            "name": s.name,
                            "transport": s.transport,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Per-hook-event details: {event, handler_count, handlers: [{type, command?, prompt?, timeout, matcher?}]}
        let mut hook_event_details: HashMap<String, (usize, Vec<Value>)> = HashMap::new();
        for entry in &hook_config.hooks {
            let event_name = serde_json::to_string(&entry.event)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            let handlers: Vec<Value> = entry
                .hooks
                .iter()
                .map(|h| {
                    let mut handler_json = json!({
                        "type": h.handler_type,
                        "timeout": h.timeout,
                    });
                    if let Some(ref cmd) = h.command {
                        handler_json["command"] = json!(cmd);
                    }
                    if let Some(ref prompt) = h.prompt {
                        handler_json["prompt"] = json!(prompt);
                    }
                    if let Some(ref matcher) = entry.matcher {
                        handler_json["matcher"] = json!(matcher);
                    }
                    handler_json
                })
                .collect();
            let entry_detail = hook_event_details
                .entry(event_name)
                .or_insert_with(|| (0, Vec::new()));
            entry_detail.0 += entry.hooks.len();
            entry_detail.1.extend(handlers);
        }
        let hooks: Vec<Value> = hook_event_details
            .into_iter()
            .map(|(event, (handler_count, handlers))| {
                json!({
                    "event": event,
                    "handler_count": handler_count,
                    "handlers": handlers,
                })
            })
            .collect();

        plugins.push(json!({
            "name": plugin.manifest.name,
            "version": plugin.manifest.version,
            "description": plugin.manifest.description,
            "source": plugin.source,
            "path": plugin.path.display().to_string(),
            "skill_count": skill_count,
            "mcp_server_count": mcp_server_count,
            "hook_count": hook_count,
            "has_settings": has_settings,
            "skills": skills,
            "mcp_servers": mcp_servers,
            "hooks": hooks,
        }));
    }
    Ok(json!({ "plugins": plugins }))
}

// --- plugins.reload ---

async fn handle_plugins_reload(state: &AppState) -> Result<Value, JsonRpcError> {
    match super::skills::reload_skills_into_orchestrator(&state.orchestrator).await {
        Ok(count) => {
            let home = dirs::home_dir().unwrap_or_default();
            let mut plugin_manager = loom_plugins::PluginManager::new();
            let n = tokio::task::spawn_blocking(move || plugin_manager.discover(&home))
                .await
                .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?
                .unwrap_or(0);
            if n > 0 {
                state
                    .orchestrator
                    .load_hooks_from_plugins(&plugin_manager)
                    .await;
            }
            tracing::info!("[plugins.reload] {} skills reloaded, {} plugins", count, n);
            Ok(json!({ "ok": true, "skill_count": count, "plugin_count": n }))
        }
        Err(e) => Err(err(ErrorCode::InternalError, &e)),
    }
}
