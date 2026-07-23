//! System dispatch handlers — system.health / agent.* / tools.list / config.*

use loom_types::{AgentConfig, ErrorCode, JsonRpcError, SandboxConfig};
use serde_json::{Value, json};
use std::path::PathBuf;

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        // System
        "system.health" => Some(handle_system_health(state).await),
        // Agent
        "agent.list" => Some(handle_agent_list(state).await),
        "agent.status" => Some(handle_agent_status(state, p).await),
        "agent.kill" => Some(handle_agent_kill(state, p).await),
        "agent.config.list" => Some(handle_agent_config_list(state).await),
        "agent.config.get" => Some(handle_agent_config_get(state, p).await),
        "agent.config.create" => Some(handle_agent_config_create(state, p).await),
        "agent.config.update" => Some(handle_agent_config_update(state, p).await),
        "agent.config.delete" => Some(handle_agent_config_delete(state, p).await),
        "agent.config.generate" => Some(handle_agent_config_generate(state, p).await),
        "agent.config.optimize" => Some(handle_agent_config_optimize(state, p).await),
        // Tools
        "tools.list" => Some(handle_tools_list(state).await),
        // Workspace
        "workspace.git_remote" => Some(handle_workspace_git_remote(p).await),
        // Config (vision / auxiliary / fim)
        "config.get_vision" => Some(handle_config_get_vision(state).await),
        "config.set_vision" => Some(handle_config_set_vision(state, p).await),
        "config.get_auxiliary" => Some(handle_config_get_auxiliary(state).await),
        "config.set_auxiliary" => Some(handle_config_set_auxiliary(state, p).await),
        "config.get_fim" => Some(handle_config_get_fim(state).await),
        "config.set_fim" => Some(handle_config_set_fim(state, p).await),
        // Sandbox
        "config.get_sandbox" => Some(handle_config_get_sandbox(state).await),
        "config.set_sandbox" => Some(handle_config_set_sandbox(state, p).await),
        // Tool prefs
        "config.get_tool_prefs" => Some(handle_config_get_tool_prefs(state).await),
        "config.set_tool_prefs" => Some(handle_config_set_tool_prefs(state, p).await),
        // Global defaults
        "config.get_defaults" => Some(handle_config_get_defaults(state).await),
        "config.set_defaults" => Some(handle_config_set_defaults(state, p).await),
        // Loom.md — 全局 Agent 纪律文件（读取/编辑）
        "loom_md.read" => Some(handle_loom_md_read(p)),
        "loom_md.save" => Some(handle_loom_md_save(p)),
        _ => None,
    }
}

// --- system.health ---

async fn handle_system_health(state: &AppState) -> Result<Value, JsonRpcError> {
    let agent_configs = state.orchestrator.agent_config_list().await.len();
    let active_agents = state.orchestrator.list_agents().await.len();
    Ok(json!({
        "status": "ok", "version": "0.2.18",
        "agent_count": agent_configs,
        "active_agent_count": active_agents,
        "tool_count": state.orchestrator.tool_registry().await.len(),
    }))
}

// --- agent.list ---

async fn handle_agent_list(state: &AppState) -> Result<Value, JsonRpcError> {
    Ok(json!({ "agents": state.orchestrator.list_agents().await }))
}

// --- agent.status ---

async fn handle_agent_status(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
    state
        .orchestrator
        .agent_status(&loom_types::AgentId::from(id))
        .await
        .map(|s| serde_json::to_value(s).unwrap_or_default())
        .map_err(|e| err(ErrorCode::AgentNotFound, &e.to_string()))
}

// --- agent.kill ---

async fn handle_agent_kill(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
    state
        .orchestrator
        .kill_agent(&loom_types::AgentId::from(id))
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- agent.config.list ---

async fn handle_agent_config_list(state: &AppState) -> Result<Value, JsonRpcError> {
    let mut configs = state.orchestrator.agent_config_list().await;
    // Filter out internal __team_ synthetic configs — they should not appear in
    // user-facing agent selectors or the agent config settings panel.
    configs.retain(|c| !c.name.starts_with("__team_captain_") && !c.name.starts_with("__team_"));
    Ok(json!({ "configs": configs }))
}

// --- agent.config.get ---

async fn handle_agent_config_get(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("default");
    let config = state
        .orchestrator
        .agent_config_get(name)
        .await
        .map_err(|e| err(ErrorCode::AgentNotFound, &e.to_string()))?;
    Ok(serde_json::to_value(config).unwrap_or_default())
}

// --- agent.config.create ---

async fn handle_agent_config_create(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let config: AgentConfig = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    if config.name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    state
        .orchestrator
        .agent_config_create(config)
        .await
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- agent.config.update ---

async fn handle_agent_config_update(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let config: AgentConfig = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    if config.name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    let new_name = config.name.clone();
    let prev_name = p
        .get("prev_name")
        .and_then(|v| v.as_str())
        .unwrap_or(&new_name)
        .to_string();
    let cache_size = state.orchestrator.agent_config_list().await.len();
    tracing::info!(
        new_name = %new_name,
        prev_name = %prev_name,
        cache_size,
        "agent.config.update"
    );
    state
        .orchestrator
        .agent_config_update(config, &prev_name)
        .await
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- agent.config.delete ---

async fn handle_agent_config_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    if name == "default" {
        return Err(err(
            ErrorCode::InvalidRequest,
            "cannot delete default config",
        ));
    }
    state
        .orchestrator
        .agent_config_delete(name)
        .await
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- agent.config.generate ---

async fn handle_agent_config_generate(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let description = p.get("description").and_then(|v| v.as_str()).unwrap_or("");
    if description.trim().is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "description required"));
    }
    let config = state
        .orchestrator
        .agent_config_generate(description.trim())
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(serde_json::to_value(config).unwrap_or_default())
}

// --- agent.config.optimize ---

async fn handle_agent_config_optimize(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let current = p
        .get("current")
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "current config required"))?;
    let current_config: loom_types::AgentConfig =
        serde_json::from_value(current.clone()).map_err(|e| {
            err(
                ErrorCode::InvalidRequest,
                &format!("invalid current config: {}", e),
            )
        })?;
    let instructions = p
        .get("instructions")
        .and_then(|v| v.as_str())
        .unwrap_or("优化此 Agent 的 persona 和系统提示词");
    let config = state
        .orchestrator
        .agent_config_optimize(current_config, instructions.trim())
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(serde_json::to_value(config).unwrap_or_default())
}

// --- tools.list ---

async fn handle_tools_list(state: &AppState) -> Result<Value, JsonRpcError> {
    let tools: Vec<serde_json::Value> = state
        .orchestrator
        .tool_registry()
        .await
        .all_definitions()
        .into_iter()
        .map(|t| json!({ "name": t.name, "description": t.description }))
        .collect();
    Ok(json!({ "tools": tools }))
}

// --- workspace.git_remote ---

async fn handle_workspace_git_remote(p: &Value) -> Result<Value, JsonRpcError> {
    let workspace = p.get("workspace").and_then(|v| v.as_str()).unwrap_or("");
    if workspace.is_empty() {
        return Ok(json!({ "url": null }));
    }
    let output = std::process::Command::new("git")
        .args(["-C", workspace, "remote", "get-url", "origin"])
        .output()
        .ok();
    let url = output.and_then(|o| {
        if o.status.success() {
            String::from_utf8(o.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            None
        }
    });
    Ok(json!({ "url": url }))
}

// --- config.get_vision ---

async fn handle_config_get_vision(state: &AppState) -> Result<Value, JsonRpcError> {
    let vision = state.orchestrator.config_store().vision().await;
    Ok(json!({ "enabled": vision.enabled, "model": vision.model }))
}

// --- config.set_vision ---

async fn handle_config_set_vision(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let enabled = p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let model = p.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
    let vision = loom_types::config::unified::VisionConfig { enabled, model };
    state
        .orchestrator
        .config_store()
        .save_vision(vision)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &format!("Write error: {}", e)))?;
    Ok(json!({ "ok": true }))
}

// --- config.get_auxiliary ---

async fn handle_config_get_auxiliary(state: &AppState) -> Result<Value, JsonRpcError> {
    let aux = state.orchestrator.config_store().auxiliary().await;
    Ok(json!({ "summary_model": aux.summary_model, "entity_model": aux.entity_model }))
}

// --- config.set_auxiliary ---

async fn handle_config_set_auxiliary(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let summary_model = p.get("summary_model").and_then(|v| v.as_str()).map(|s| s.to_string());
    let entity_model = p.get("entity_model").and_then(|v| v.as_str()).map(|s| s.to_string());
    let aux = loom_types::config::unified::AuxiliaryConfig {
        summary_model,
        entity_model,
    };
    state
        .orchestrator
        .config_store()
        .save_auxiliary(aux)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &format!("Write error: {}", e)))?;
    Ok(json!({ "ok": true }))
}

// --- config.get_fim ---

async fn handle_config_get_fim(state: &AppState) -> Result<Value, JsonRpcError> {
    let fim = state.orchestrator.config_store().fim().await;
    Ok(json!({ "model": fim.model, "base_url": fim.base_url, "api_key_env": fim.api_key_env }))
}

// --- config.set_fim ---

async fn handle_config_set_fim(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let existing = state.orchestrator.config_store().fim().await;
    let model = if p.get("model").is_some() {
        p.get("model").and_then(|v| v.as_str()).map(|s| s.to_string())
    } else {
        existing.model
    };
    let base_url = if p.get("base_url").is_some() {
        p.get("base_url").and_then(|v| v.as_str()).map(|s| s.to_string())
    } else {
        existing.base_url
    };
    let api_key_env = if p.get("api_key_env").is_some() {
        p.get("api_key_env").and_then(|v| v.as_str()).map(|s| s.to_string())
    } else {
        existing.api_key_env
    };
    let fim = loom_types::config::unified::FimConfig {
        model,
        base_url,
        api_key_env,
    };
    state
        .orchestrator
        .config_store()
        .save_fim(fim)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &format!("Write error: {}", e)))?;
    Ok(json!({ "ok": true }))
}

// --- config.get_sandbox ---

async fn handle_config_get_sandbox(state: &AppState) -> Result<Value, JsonRpcError> {
    let config = state.orchestrator.load_sandbox_config().await;
    Ok(serde_json::to_value(config).unwrap_or_default())
}

// --- config.set_sandbox ---

async fn handle_config_set_sandbox(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let config: SandboxConfig = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    state
        .orchestrator
        .save_sandbox_config(&config)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- config.get_defaults ---

async fn handle_config_get_defaults(state: &AppState) -> Result<Value, JsonRpcError> {
    let max_iterations = state.orchestrator.get_default_max_iterations().await;
    let max_prompt_budget = state.orchestrator.get_default_max_prompt_budget().await;
    Ok(json!({
        "max_iterations": max_iterations,
        "max_prompt_budget": max_prompt_budget,
    }))
}

// --- config.set_defaults ---

async fn handle_config_set_defaults(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    if let Some(v) = p.get("max_iterations").and_then(|v| v.as_u64()) {
        state
            .orchestrator
            .set_default_max_iterations(v as usize)
            .await;
    }
    if let Some(v) = p.get("max_prompt_budget").and_then(|v| v.as_u64()) {
        state
            .orchestrator
            .set_default_max_prompt_budget(v as usize)
            .await;
    }
    Ok(json!({ "ok": true }))
}

// --- config.get_tool_prefs ---

async fn handle_config_get_tool_prefs(state: &AppState) -> Result<Value, JsonRpcError> {
    let config = state.orchestrator.load_tool_prefs().await;
    Ok(serde_json::to_value(config).unwrap_or_default())
}

// --- config.set_tool_prefs ---

async fn handle_config_set_tool_prefs(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let mut config = state.orchestrator.load_tool_prefs().await;
    if let Some(v) = p.get("shell_default_timeout_secs").and_then(|v| v.as_u64()) {
        config.shell_default_timeout_secs = v;
    }
    if let Some(v) = p.get("shell_max_timeout_secs").and_then(|v| v.as_u64()) {
        config.shell_max_timeout_secs = v;
    }
    if let Some(v) = p.get("file_read_max_output_kb").and_then(|v| v.as_u64()) {
        config.file_read_max_output_kb = v as usize;
    }
    if let Some(v) = p.get("web_search_engine").and_then(|v| v.as_str()) {
        config.web_search_engine =
            serde_json::from_value(serde_json::Value::String(v.to_string())).unwrap_or_default();
    }
    if let Some(v) = p.get("web_search_max_results").and_then(|v| v.as_u64()) {
        config.web_search_max_results = v as usize;
    }
    if let Some(v) = p.get("searxng_url").and_then(|v| v.as_str()) {
        config.searxng_url = if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        };
    }
    if let Some(v) = p.get("web_search_api_key").and_then(|v| v.as_str()) {
        config.web_search_api_key = if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        };
    }
    if let Some(v) = p.get("http_proxy").and_then(|v| v.as_str()) {
        config.http_proxy = if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        };
    }
    if let Some(v) = p.get("proxy_enabled").and_then(|v| v.as_bool()) {
        config.proxy_enabled = v;
    }
    if let Some(v) = p.get("web_fetch_max_chars").and_then(|v| v.as_u64()) {
        config.web_fetch_max_chars = v as usize;
    }
    if let Some(v) = p
        .get("process_wait_max_timeout_secs")
        .and_then(|v| v.as_u64())
    {
        config.process_wait_max_timeout_secs = v;
    }
    if let Some(v) = p.get("monitor_default_timeout_ms").and_then(|v| v.as_u64()) {
        config.monitor_default_timeout_ms = v;
    }
    state
        .orchestrator
        .save_tool_prefs(&config)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    // save_tool_prefs already updates the in-memory state and syncs GLOBAL_PROXY.
    Ok(json!({ "ok": true }))
}

// --- loom_md.read ---
//
// 读取 Loom.md 原文（含空内容），供设置面板编辑器展示。
// - 无 workspace_root → 读取全局 ~/.loom/Loom.md，文件不存在时返回空串
// - 有 workspace_root → 读取 $workspace_root/Loom.md，文件不存在时自动创建空文件

fn handle_loom_md_read(p: &Value) -> Result<Value, JsonRpcError> {
    let workspace = p.get("workspace_root").and_then(|v| v.as_str());
    if let Some(ws) = workspace {
        let loom_file = PathBuf::from(ws).join("Loom.md");
        let content = std::fs::read_to_string(&loom_file).unwrap_or_default();
        Ok(json!({ "content": content, "path": loom_file.display().to_string() }))
    } else {
        let home = dirs::home_dir().unwrap_or_default().join(".loom");
        let loom_file = home.join("Loom.md");
        let content = std::fs::read_to_string(&loom_file).unwrap_or_default();
        Ok(json!({ "content": content, "path": loom_file.display().to_string() }))
    }
}

// --- loom_md.save ---
//
// 写入 Loom.md。
// - 无 workspace_root → 写入全局 ~/.loom/Loom.md
// - 有 workspace_root → 写入 $workspace_root/Loom.md

fn handle_loom_md_save(p: &Value) -> Result<Value, JsonRpcError> {
    let content = p.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let workspace = p.get("workspace_root").and_then(|v| v.as_str());
    let loom_file = if let Some(ws) = workspace {
        let f = PathBuf::from(ws).join("Loom.md");
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        f
    } else {
        let home = dirs::home_dir().unwrap_or_default().join(".loom");
        let _ = std::fs::create_dir_all(&home);
        home.join("Loom.md")
    };
    std::fs::write(&loom_file, content)
        .map_err(|e| err(ErrorCode::InternalError, &format!("Write error: {}", e)))?;
    Ok(json!({ "ok": true, "path": loom_file.display().to_string() }))
}
