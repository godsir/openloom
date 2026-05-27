//! Tool/function calling types shared across skills, engine, and inference.
//!
//! Consumers: loom-core (agent loop, ToolRegistry), loom-inference, loom-server (dispatch), loom-mcp

use serde::{Deserialize, Serialize};

/// Tool definition sent to the LLM in the `tools` parameter of a completion request.
///
/// Consumers: loom-core (build_tool_definitions), loom-inference (CompletionRequest), loom-mcp (tool discovery)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A parsed tool call from a model response.
///
/// Consumers: loom-core (agent loop dispatch), loom-inference (response parsing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Controls whether the model must call a tool.
///
/// Consumers: loom-core (agent loop), loom-inference (CompletionRequest)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    Auto,
    None,
    Required,
}

/// Fine-grained permission scope for tool execution.
///
/// Consumers: loom-core (agent loop permission check), loom-server (dispatch), loom-security
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolScope {
    None,
    ReadOnly,
    Selective,
    Full,
}

impl ToolScope {
    /// Check whether a tool name is within this scope.
    pub fn allows(&self, tool_name: &str) -> bool {
        match self {
            ToolScope::Full => true,
            ToolScope::None => false,
            ToolScope::ReadOnly => READ_ONLY_TOOLS.contains(&tool_name),
            ToolScope::Selective => SELECTIVE_TOOLS.contains(&tool_name),
        }
    }
}

const READ_ONLY_TOOLS: &[&str] = &["file_read", "file_search", "content_search", "web_browser"];

const SELECTIVE_TOOLS: &[&str] = &[
    "file_read",
    "file_search",
    "content_search",
    "web_browser",
    "schedule_reminder",
    "file_edit",
];

/// Intermediate progress reported by a tool/skill during execution.
///
/// Consumers: loom-core (agent loop), loom-server (WS notifications), loom-skills (Skill trait)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgress {
    /// Progress fraction 0.0–1.0, None = indeterminate
    pub progress: Option<f64>,
    /// Human-readable status message
    pub message: String,
    /// Optional structured payload
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}
