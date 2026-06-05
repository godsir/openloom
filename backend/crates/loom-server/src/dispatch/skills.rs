//! Skills dispatch handlers — skills.* + reload_skills_into_orchestrator

use std::sync::Arc;

use loom_skills::SkillLoader;
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
        "skills.list" => Some(handle_skills_list(state).await),
        "skills.get" => Some(handle_skills_get(state, p).await),
        "skills.import" => Some(handle_skills_import(state, p).await),
        "skills.delete" => Some(handle_skills_delete(state, p).await),
        _ => None,
    }
}

// --- skills.list ---

async fn handle_skills_list(state: &AppState) -> Result<Value, JsonRpcError> {
    let summaries = state.orchestrator.get_skill_summaries().await;
    let list: Vec<Value> = summaries
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "description": s.description,
                "path": s.source_path,
                "version": s.version,
                "user_invocable": s.user_invocable,
                "always_active": s.always_active,
            })
        })
        .collect();
    Ok(json!({ "skills": list }))
}

// --- skills.get ---

async fn handle_skills_get(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    match state.orchestrator.get_skill_body(name).await {
        Some(content) => Ok(json!({ "content": content })),
        None => Err(err(
            ErrorCode::MethodNotFound,
            &format!("skill '{}' not found", name),
        )),
    }
}

// --- skills.import ---

async fn handle_skills_import(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    // Import a skill: write files to ~/.loom/skills/<name>/
    // Accepts: { name: string, files: [{ path: string, content: string }] }
    // Minimum: at least one file named SKILL.md
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    let files = p.get("files").and_then(|v| v.as_array());
    if files.is_none() || files.unwrap().is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "files array required"));
    }
    let home = dirs::home_dir().unwrap_or_default();
    let skill_dir = home.join(".loom").join("skills").join(name);
    std::fs::create_dir_all(&skill_dir)
        .map_err(|e| err(ErrorCode::InternalError, &format!("mkdir failed: {}", e)))?;

    let mut wrote = 0usize;
    for file in files.unwrap() {
        let rel_path = file.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = file.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if rel_path.is_empty() {
            continue;
        }
        // Prevent path traversal
        if rel_path.contains("..") {
            continue;
        }
        let target = skill_dir.join(rel_path);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&target, content)
            .map_err(|e| err(ErrorCode::InternalError, &format!("write failed: {}", e)))?;
        wrote += 1;
    }
    // Refresh orchestrator skill state
    let _ = reload_skills_into_orchestrator(&state.orchestrator).await;
    Ok(json!({ "ok": true, "path": skill_dir.display().to_string(), "files_written": wrote }))
}

// --- skills.delete ---

async fn handle_skills_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    let home = dirs::home_dir().unwrap_or_default();
    let skill_dir = home.join(".loom").join("skills").join(name);
    if skill_dir.exists() {
        std::fs::remove_dir_all(&skill_dir)
            .map_err(|e| err(ErrorCode::InternalError, &format!("delete failed: {}", e)))?;
    }
    // Refresh orchestrator skill state
    let _ = reload_skills_into_orchestrator(&state.orchestrator).await;
    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Shared helper: reload skills into orchestrator
// Used by: skills.import, skills.delete, plugins.reload, marketplace.* handlers
// ---------------------------------------------------------------------------

/// Reload skills from all standard paths into the orchestrator.
/// Used by plugins.reload, skills.import, and skills.delete to keep
/// orchestrator skill state in sync with the filesystem.
pub(crate) async fn reload_skills_into_orchestrator(
    orchestrator: &Arc<loom_core::Orchestrator>,
) -> Result<usize, String> {
    let home = dirs::home_dir().unwrap_or_default();
    let data_dir = home.join(".loom");
    let mut loader = SkillLoader::new();
    loader.add_standard_paths(&data_dir);

    // Also discover plugins and feed their skill paths to the loader
    let mut plugin_manager = loom_plugins::PluginManager::new();
    if let Ok(n) = plugin_manager.discover(&home)
        && n > 0
    {
        tracing::info!(plugin_count = n, "plugins discovered during skill reload");
        for path in plugin_manager.skill_paths() {
            if path.exists() {
                loader.add_path(path, "plugin");
            }
        }
    }

    // Reconnect plugin MCP servers — kept in sync with plugin manifests
    let mcp_configs = plugin_manager.mcp_configs();
    if !mcp_configs.is_empty() {
        let orch = orchestrator.clone();
        tokio::spawn(async move {
            for mcp in mcp_configs {
                let config = loom_mcp::McpServerConfig {
                    name: mcp.name.clone(),
                    transport: mcp.transport.clone(),
                    command: mcp.command.clone(),
                    args: mcp.args.clone(),
                    url: mcp.url.clone(),
                    headers: mcp.headers.clone(),
                    env: mcp.env.clone(),
                    cwd: None,
                    startup_timeout_secs: 10,
                    tool_timeout_secs: 60,
                    enabled_tools: None,
                    disabled_tools: None,
                };
                match orch.connect_mcp_server(config).await {
                    Ok(_) => tracing::info!(server = %mcp.name, "plugin MCP server reconnected"),
                    Err(e) => {
                        tracing::warn!(server = %mcp.name, error = %e, "plugin MCP server reconnect failed")
                    }
                }
            }
        });
    }

    // Reload hook registry from plugins after MCP reconnection
    orchestrator.reload_hooks(&plugin_manager).await;

    match loader.discover() {
        Ok(skills) => {
            let count = skills.len();
            let state = loom_skills::SkillState::from_skills(&skills);
            orchestrator.set_skills(state).await;
            Ok(count)
        }
        Err(e) => Err(format!("skill discovery failed: {}", e)),
    }
}
