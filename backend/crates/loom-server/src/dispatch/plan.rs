//! Plan and todo JSON-RPC handlers.
//!
//! Plans are stored as .md files under .loom/plans/ with a companion
//! .meta.json file for metadata. This survives restarts.

use crate::AppState;
use anyhow::Result;
use loom_types::plan::*;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::err;
use loom_types::ErrorCode;

/// Handle plan.* and todo.* JSON-RPC methods.
pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, loom_types::JsonRpcError>> {
    match method {
        "plan.create" => Some(handle_plan_create(state, p).await),
        "plan.get" => Some(handle_plan_get(state, p).await),
        "plan.list" => Some(handle_plan_list(state, p).await),
        "plan.update" => Some(handle_plan_update(state, p).await),
        "plan.delete" => Some(handle_plan_delete(state, p).await),
        "todo.list" => Some(handle_todo_list(state, p).await),
        "todo.update_status" => Some(handle_todo_update_status(state, p).await),
        "todo.clear" => Some(handle_todo_clear(state, p).await),
        "goal.set" => Some(handle_goal_set(state, p).await),
        "goal.status" => Some(handle_goal_status(state, p).await),
        _ => None,
    }
}

fn plans_dir(workspace_root: &str) -> PathBuf {
    PathBuf::from(workspace_root).join(".loom").join("plans")
}

fn meta_for_plan(workspace_root: &str, id: &str) -> PathBuf {
    plans_dir(workspace_root).join(format!("{}.meta.json", id))
}

async fn handle_plan_create(
    _state: &AppState,
    p: &Value,
) -> Result<Value, loom_types::JsonRpcError> {
    let req: CreatePlanRequest = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;

    let id = uuid::Uuid::new_v4().to_string();
    let relative_path = format!(".loom/plans/{}.md", id);
    let now = chrono::Utc::now().to_rfc3339();

    let plan = PlanArtifact {
        id: id.clone(),
        workspace_root: req.workspace_root.clone(),
        thread_id: req.thread_id,
        title: format!("Plan {}", &id[..8]),
        relative_path: relative_path.clone(),
        source_request: req.request.clone(),
        status: PlanStatus::Drafting,
        created_at: now.clone(),
        updated_at: now,
    };

    // Persist to filesystem
    let dir = plans_dir(&req.workspace_root);
    std::fs::create_dir_all(&dir).ok();
    if let Ok(meta_json) = serde_json::to_string_pretty(&plan) {
        std::fs::write(meta_for_plan(&req.workspace_root, &id), meta_json).ok();
    }

    // In-memory cache
    PLANS.write().await.insert(id.clone(), plan.clone());

    Ok(serde_json::to_value(&plan).map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?)
}

fn plan_md_path(workspace_root: &str, id: &str) -> PathBuf {
    plans_dir(workspace_root).join(format!("{}.md", id))
}

/// Extract `- [ ]` / `- [x]` checkboxes from plan markdown and sync to the todo store.
async fn sync_checkboxes_to_todos(state: &AppState, content: &str, thread_id: &str, plan_id: &str) {
    let mut todos: Vec<loom_memory::TodoItem> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.len() < 5 {
            continue;
        }
        let prefix = &trimmed[..3];
        if prefix != "- [" && prefix != "* [" {
            continue;
        }
        let after = &trimmed[3..];
        if !after.starts_with("] ") {
            continue;
        }
        let checked = &trimmed[1..4] != "- [ "; // "- [x]" or "- [X]"
        let text = trimmed[5..].trim();
        if text.is_empty() {
            continue;
        }
        let status = if checked { "completed" } else { "pending" };
        todos.push(loom_memory::TodoItem {
            id: uuid::Uuid::now_v7().to_string(),
            session_id: thread_id.to_string(),
            content: text.to_string(),
            status: status.to_string(),
            plan_id: Some(plan_id.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
    }
    if !todos.is_empty() {
        let _ = state.orchestrator.replace_todos(thread_id, &todos).await;
    }
}

async fn handle_plan_get(state: &AppState, p: &Value) -> Result<Value, loom_types::JsonRpcError> {
    let plan_id = p.get("plan_id").and_then(|v| v.as_str()).unwrap_or("");

    let plans = PLANS.read().await;
    let plan = plans.get(plan_id).ok_or_else(|| {
        err(
            ErrorCode::InternalError,
            &format!("plan {} not found", plan_id),
        )
    })?;

    // Read plan markdown content from filesystem
    let md_path = plan_md_path(&plan.workspace_root, plan_id);
    let content = std::fs::read_to_string(&md_path).unwrap_or_default();

    // Sync checkboxes → todo list (same logic as plan.update)
    if !content.is_empty() {
        if let Some(ref thread_id) = plan.thread_id {
            sync_checkboxes_to_todos(state, &content, thread_id, plan_id).await;
        }
    }

    Ok(serde_json::json!({ "plan": plan, "content": content }))
}

async fn handle_plan_list(_state: &AppState, p: &Value) -> Result<Value, loom_types::JsonRpcError> {
    let workspace = p
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Scan filesystem for plan metadata
    let mut list: Vec<PlanArtifact> = Vec::new();
    let dir = plans_dir(workspace);
    if dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".meta.json") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Ok(plan) = serde_json::from_str::<PlanArtifact>(&content) {
                            list.push(plan);
                        }
                    }
                }
            }
        }
    }

    // Also include in-memory plans not yet persisted
    let mem_plans = PLANS.read().await;
    for plan in mem_plans.values() {
        if plan.workspace_root == workspace && !list.iter().any(|p| p.id == plan.id) {
            list.push(plan.clone());
        }
    }

    // Deduplicate and sort by updated_at desc
    list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(serde_json::to_value(&list).map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?)
}

async fn handle_plan_update(
    state: &AppState,
    p: &Value,
) -> Result<Value, loom_types::JsonRpcError> {
    let plan_id = p.get("plan_id").and_then(|v| v.as_str()).unwrap_or("");
    let mut plans = PLANS.write().await;
    let plan = plans.get_mut(plan_id).ok_or_else(|| {
        err(
            ErrorCode::InternalError,
            &format!("plan {} not found", plan_id),
        )
    })?;

    if let Some(status) = p.get("status").and_then(|v| v.as_str()) {
        plan.status = serde_json::from_value(serde_json::Value::String(status.into()))
            .unwrap_or(PlanStatus::Drafting);
    }
    plan.updated_at = chrono::Utc::now().to_rfc3339();

    // Persist metadata
    if let Ok(meta_json) = serde_json::to_string_pretty(&*plan) {
        std::fs::write(meta_for_plan(&plan.workspace_root, &plan_id), meta_json).ok();
    }

    // Persist plan content + sync checkboxes → todos
    if let Some(content) = p.get("content").and_then(|v| v.as_str()) {
        let plan_path = plans_dir(&plan.workspace_root).join(format!("{}.md", plan_id));
        std::fs::write(&plan_path, content).ok();

        // Sync plan checkboxes to the todo list
        if let Some(ref thread_id) = plan.thread_id {
            sync_checkboxes_to_todos(state, content, thread_id, plan_id).await;
        }
    }

    Ok(serde_json::to_value(&plan).map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?)
}

async fn handle_plan_delete(
    _state: &AppState,
    p: &Value,
) -> Result<Value, loom_types::JsonRpcError> {
    let plan_id = p.get("plan_id").and_then(|v| v.as_str()).unwrap_or("");
    let plan = PLANS.write().await.remove(plan_id);
    if let Some(plan) = &plan {
        // Remove metadata file
        std::fs::remove_file(meta_for_plan(&plan.workspace_root, plan_id)).ok();
    }
    Ok(Value::Bool(true))
}

async fn handle_todo_list(state: &AppState, p: &Value) -> Result<Value, loom_types::JsonRpcError> {
    let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let todos = state
        .orchestrator
        .list_todos(session_id)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(serde_json::to_value(&todos).map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?)
}

async fn handle_todo_update_status(
    state: &AppState,
    p: &Value,
) -> Result<Value, loom_types::JsonRpcError> {
    let req: UpdateTodoStatusRequest = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    let status_str = match req.status {
        loom_types::plan::TodoStatus::Pending => "pending",
        loom_types::plan::TodoStatus::InProgress => "in_progress",
        loom_types::plan::TodoStatus::Completed => "completed",
    };
    state
        .orchestrator
        .update_todo_status(&req.session_id, &req.todo_id, status_str)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(Value::Bool(true))
}

async fn handle_todo_clear(state: &AppState, p: &Value) -> Result<Value, loom_types::JsonRpcError> {
    let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    state
        .orchestrator
        .clear_todos(session_id)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(Value::Bool(true))
}

async fn handle_goal_set(_state: &AppState, p: &Value) -> Result<Value, loom_types::JsonRpcError> {
    let req: SetGoalRequest = serde_json::from_value(p.clone())
        .map_err(|e| err(ErrorCode::InvalidRequest, &e.to_string()))?;
    let goal = ThreadGoal {
        session_id: req.session_id,
        description: req.description,
        status: GoalStatus::Active,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let mut goals = GOALS.write().await;
    goals.insert(goal.session_id.clone(), goal.clone());
    Ok(serde_json::to_value(&goal).map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?)
}

async fn handle_goal_status(
    _state: &AppState,
    p: &Value,
) -> Result<Value, loom_types::JsonRpcError> {
    let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    let goals = GOALS.read().await;
    let goal = goals.get(session_id).cloned();
    Ok(serde_json::to_value(&goal).map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?)
}

// In-memory storage for active sessions
static PLANS: std::sync::LazyLock<Arc<RwLock<HashMap<String, PlanArtifact>>>> =
    std::sync::LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));
static GOALS: std::sync::LazyLock<Arc<RwLock<HashMap<String, ThreadGoal>>>> =
    std::sync::LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));
