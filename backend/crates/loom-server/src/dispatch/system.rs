//! System dispatch handlers — system.health / agent.* / tools.list / config.* / marketplace.*

use loom_types::{AgentConfig, ErrorCode, JsonRpcError, SandboxConfig};
use serde_json::{Value, json};
use std::path::Path;

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
        // Config (vision / auxiliary)
        "config.get_vision" => Some(handle_config_get_vision()),
        "config.set_vision" => Some(handle_config_set_vision(p)),
        "config.get_auxiliary" => Some(handle_config_get_auxiliary()),
        "config.set_auxiliary" => Some(handle_config_set_auxiliary(p)),
        // Sandbox
        "config.get_sandbox" => Some(handle_config_get_sandbox(state).await),
        "config.set_sandbox" => Some(handle_config_set_sandbox(state, p).await),
        // Global defaults
        "config.get_defaults" => Some(handle_config_get_defaults(state).await),
        "config.set_defaults" => Some(handle_config_set_defaults(state, p).await),
        // Marketplace
        "marketplace.list" => Some(handle_marketplace_list(state, p).await),
        "marketplace.install" => Some(handle_marketplace_install(state, p).await),
        "marketplace.uninstall" => Some(handle_marketplace_uninstall(state, p).await),
        "marketplace.update" => Some(handle_marketplace_update(state, p).await),
        _ => None,
    }
}

// --- system.health ---

async fn handle_system_health(state: &AppState) -> Result<Value, JsonRpcError> {
    Ok(json!({
        "status": "ok", "version": "0.2.18",
        "agent_count": state.orchestrator.list_agents().await.len(),
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
    let configs = state.orchestrator.agent_config_list().await;
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
    let names = state.orchestrator.tool_registry().await.list_names();
    Ok(json!({ "tools": names }))
}

// --- config.get_vision ---

fn handle_config_get_vision() -> Result<Value, JsonRpcError> {
    let home = dirs::home_dir().unwrap_or_default().join(".loom");
    let config_file = home.join("vision.json");
    let config: Value = std::fs::read_to_string(&config_file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(json!({ "enabled": false, "model": null }));
    Ok(config)
}

// --- config.set_vision ---

fn handle_config_set_vision(p: &Value) -> Result<Value, JsonRpcError> {
    let enabled = p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let model = p
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let home = dirs::home_dir().unwrap_or_default().join(".loom");
    let _ = std::fs::create_dir_all(&home);
    let config = json!({ "enabled": enabled, "model": model });
    let config_file = home.join("vision.json");
    std::fs::write(
        &config_file,
        serde_json::to_string_pretty(&config).unwrap_or_default(),
    )
    .map_err(|e| err(ErrorCode::InternalError, &format!("Write error: {}", e)))?;
    Ok(json!({ "ok": true }))
}

// --- config.get_auxiliary ---

fn handle_config_get_auxiliary() -> Result<Value, JsonRpcError> {
    let home = dirs::home_dir().unwrap_or_default().join(".loom");
    let config_file = home.join("auxiliary.json");
    let config: Value = std::fs::read_to_string(&config_file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(json!({ "summary_model": null, "entity_model": null }));
    Ok(config)
}

// --- config.set_auxiliary ---

fn handle_config_set_auxiliary(p: &Value) -> Result<Value, JsonRpcError> {
    let summary_model = p
        .get("summary_model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let entity_model = p
        .get("entity_model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let home = dirs::home_dir().unwrap_or_default().join(".loom");
    let _ = std::fs::create_dir_all(&home);
    let config = json!({ "summary_model": summary_model, "entity_model": entity_model });
    let config_file = home.join("auxiliary.json");
    std::fs::write(
        &config_file,
        serde_json::to_string_pretty(&config).unwrap_or_default(),
    )
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

// --- marketplace.list ---

async fn handle_marketplace_list(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let _ = state;
    let home = dirs::home_dir().unwrap_or_default();
    let plugins_dir = home.join(".loom").join("plugins");
    let skills_dir = home.join(".loom").join("skills");

    let catalog_url = p
        .get("catalog_url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    let mut all_entries: Vec<loom_marketplace::MarketPlugin> =
        loom_marketplace::default_catalog().plugins;

    // If a remote catalog URL is provided, fetch and append its entries
    if let Some(url) = catalog_url {
        if let Some(remote) = fetch_remote_catalog(url).await {
            all_entries.extend(remote.plugins);
        }
    }

    // Build results with install status
    let results = tokio::task::spawn_blocking(move || {
        all_entries
            .iter()
            .map(|entry| {
                let target_dir = match entry.kind {
                    loom_marketplace::MarketEntryKind::Plugin => &plugins_dir,
                    loom_marketplace::MarketEntryKind::Skill => &skills_dir,
                };
                let (installed, installed_version, installed_path) =
                    check_installed_status(&entry.id, target_dir);
                let has_update = installed
                    && installed_version.is_some()
                    && version_newer(
                        &entry.version,
                        installed_version.as_deref().unwrap_or("0"),
                    );
                serde_json::json!({
                    "id": entry.id,
                    "name": entry.name,
                    "description": entry.description,
                    "version": entry.version,
                    "author": entry.author,
                    "git_url": entry.git_url,
                    "category": entry.category,
                    "kind": entry.kind.to_string(),
                    "tags": entry.tags,
                    "homepage": entry.homepage,
                    "installed": installed,
                    "has_update": has_update,
                    "installed_version": installed_version,
                    "installed_path": installed_path,
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

    Ok(json!({ "plugins": results }))
}

async fn fetch_remote_catalog(url: &str) -> Option<loom_marketplace::MarketplaceCatalog> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .ok()?;
    let resp = client
        .get(url)
        .header("User-Agent", "openLoom/0.2")
        .send()
        .await
        .ok()?;
    let body = resp.text().await.ok()?;
    serde_json::from_str::<loom_marketplace::MarketplaceCatalog>(&body).ok()
}

fn check_installed_status(entry_id: &str, target_dir: &Path) -> (bool, Option<String>, Option<String>) {
    let dir = target_dir.join(entry_id);
    if !dir.exists() || !dir.is_dir() {
        return (false, None, None);
    }
    let version = try_read_version(&dir);
    (true, version, Some(dir.to_string_lossy().to_string()))
}

fn try_read_version(entry_dir: &Path) -> Option<String> {
    // Read version.txt if present
    let version_file = entry_dir.join("version.txt");
    if let Ok(v) = std::fs::read_to_string(&version_file) {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

fn version_newer(newer: &str, older: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    let a = parse(newer);
    let b = parse(older);
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        if av > bv { return true; }
        if av < bv { return false; }
    }
    false
}

// --- marketplace.install ---

async fn handle_marketplace_install(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let entry_id = p.get("plugin_id").and_then(|v| v.as_str()).unwrap_or("");
    if entry_id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "plugin_id is required"));
    }
    let home = dirs::home_dir().unwrap_or_default();
    let plugins_dir = home.join(".loom").join("plugins");
    let skills_dir = home.join(".loom").join("skills");
    match loom_marketplace::install_from_catalog(entry_id, &plugins_dir, &skills_dir).await {
        Ok(target) => {
            let home_for_discover = home.clone();
            let (n, pm) = tokio::task::spawn_blocking(move || {
                let mut pm = loom_plugins::PluginManager::new();
                let n = pm.discover(&home_for_discover).unwrap_or(0);
                (n, pm)
            })
            .await
            .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            if n > 0 {
                state.orchestrator.load_hooks_from_plugins(&pm).await;
            }
            let _ = super::skills::reload_skills_into_orchestrator(&state.orchestrator).await;
            tracing::info!(
                "[marketplace.install] installed '{}' to {}, {} total found",
                entry_id,
                target.display(),
                n
            );
            Ok(json!({ "ok": true, "path": target.display().to_string() }))
        }
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

// --- marketplace.uninstall ---

async fn handle_marketplace_uninstall(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let entry_id = p.get("plugin_id").and_then(|v| v.as_str()).unwrap_or("");
    if entry_id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "plugin_id is required"));
    }
    let home = dirs::home_dir().unwrap_or_default();
    let plugins_dir = home.join(".loom").join("plugins");
    let skills_dir = home.join(".loom").join("skills");

    // Try both dirs — the entry could be a plugin or a skill.
    let plugin_path = plugins_dir.join(entry_id);
    let skill_path = skills_dir.join(entry_id);
    let target_dir: std::path::PathBuf = if plugin_path.exists() {
        plugins_dir.clone()
    } else if skill_path.exists() {
        skills_dir.clone()
    } else {
        return Err(err(
            ErrorCode::InternalError,
            &format!("'{}' is not installed", entry_id),
        ));
    };
    let entry_id_owned = entry_id.to_string();
    match tokio::task::spawn_blocking(move || {
        loom_marketplace::uninstall(&entry_id_owned, &target_dir)
    })
    .await
    .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?
    {
        Ok(()) => {
            let home_for_discover = home.clone();
            let (n, pm) = tokio::task::spawn_blocking(move || {
                let mut pm = loom_plugins::PluginManager::new();
                let n = pm.discover(&home_for_discover).unwrap_or(0);
                (n, pm)
            })
            .await
            .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            if n > 0 {
                state.orchestrator.load_hooks_from_plugins(&pm).await;
            }
            let _ = super::skills::reload_skills_into_orchestrator(&state.orchestrator).await;
            tracing::info!(
                "[marketplace.uninstall] uninstalled '{}', {} remaining",
                entry_id,
                n
            );
            Ok(json!({ "ok": true }))
        }
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
}

// --- marketplace.update ---

async fn handle_marketplace_update(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let entry_id = p.get("plugin_id").and_then(|v| v.as_str()).unwrap_or("");
    if entry_id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "plugin_id is required"));
    }
    let home = dirs::home_dir().unwrap_or_default();
    let plugins_dir = home.join(".loom").join("plugins");
    let skills_dir = home.join(".loom").join("skills");
    match loom_marketplace::update_from_catalog(entry_id, &plugins_dir, &skills_dir).await {
        Ok(()) => {
            let home_for_discover = home.clone();
            let (n, pm) = tokio::task::spawn_blocking(move || {
                let mut pm = loom_plugins::PluginManager::new();
                let n = pm.discover(&home_for_discover).unwrap_or(0);
                (n, pm)
            })
            .await
            .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
            if n > 0 {
                state.orchestrator.load_hooks_from_plugins(&pm).await;
            }
            let _ = super::skills::reload_skills_into_orchestrator(&state.orchestrator).await;
            tracing::info!("[marketplace.update] updated '{}'", entry_id);
            Ok(json!({ "ok": true }))
        }
        Err(e) => Err(err(ErrorCode::InternalError, &e.to_string())),
    }
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
