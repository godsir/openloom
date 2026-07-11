//! Plan and todo types for the plan/sdd/todo workflow.
//!
//! Consumers: loom-core (orchestrator, plan prompts), loom-server (dispatch)

use serde::{Deserialize, Serialize};

/// A structured implementation plan artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanArtifact {
    pub id: String,
    pub workspace_root: String,
    pub thread_id: Option<String>,
    pub title: String,
    pub relative_path: String,
    pub source_request: String,
    pub status: PlanStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Drafting,
    Ready,
    Building,
    Completed,
    Error,
}

impl Default for PlanStatus {
    fn default() -> Self {
        PlanStatus::Drafting
    }
}

/// A single todo item extracted from a plan's markdown checkboxes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    pub source: Option<TodoSource>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

impl Default for TodoStatus {
    fn default() -> Self {
        TodoStatus::Pending
    }
}

/// Source of a todo item (plan file path + ordinal + content hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoSource {
    pub plan_id: String,
    pub relative_path: String,
    pub ordinal: usize,
    pub content_hash: String,
}

/// A thread-scoped goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadGoal {
    pub session_id: String,
    pub description: String,
    pub status: GoalStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Paused,
    Completed,
}

impl Default for GoalStatus {
    fn default() -> Self {
        GoalStatus::Active
    }
}

// JSON-RPC request/response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePlanRequest {
    pub workspace_root: String,
    pub request: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePlanResponse {
    pub plan: PlanArtifact,
    pub plan_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePlanRequest {
    pub plan_id: String,
    pub content: Option<String>,
    pub status: Option<PlanStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetGoalRequest {
    pub session_id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTodoStatusRequest {
    pub session_id: String,
    pub todo_id: String,
    pub status: TodoStatus,
}
