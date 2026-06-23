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
        "skills.reload" => Some(handle_skills_reload(state).await),
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

// --- skills.reload ---

async fn handle_skills_reload(state: &AppState) -> Result<Value, JsonRpcError> {
    match reload_skills_into_orchestrator(&state.orchestrator).await {
        Ok(count) => {
            tracing::info!("[skills.reload] {} skills reloaded", count);
            Ok(json!({ "ok": true, "skill_count": count }))
        }
        Err(e) => Err(err(ErrorCode::InternalError, &e)),
    }
}

// ---------------------------------------------------------------------------
// Shared helper: reload skills into orchestrator
// Used by: skills.import, skills.delete, clawhub.* handlers
// ---------------------------------------------------------------------------

/// Reload skills from all standard paths into the orchestrator.
pub(crate) async fn reload_skills_into_orchestrator(
    orchestrator: &Arc<loom_core::Orchestrator>,
) -> Result<usize, String> {
    let home = dirs::home_dir().unwrap_or_default();
    let data_dir = home.join(".loom");
    let mut loader = SkillLoader::new();
    loader.add_standard_paths(&data_dir);

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
