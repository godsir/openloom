// Stub for codex-code-mode types.

use serde::{Deserialize, Serialize};

/// Stub enum for code mode tool kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeModeToolKind {
    Function,
    Freeform,
}

/// Stub for code-mode ToolDefinition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub tool_name: loom_protocol::ToolName,
    pub description: String,
    pub kind: CodeModeToolKind,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
}

/// The public tool name constant.
pub const PUBLIC_TOOL_NAME: &str = "exec";

/// Stub: return the definition unchanged.
pub fn augment_tool_definition(definition: ToolDefinition) -> ToolDefinition {
    definition
}

/// Stub: always returns false.
pub fn is_code_mode_nested_tool(_name: &str) -> bool {
    false
}
