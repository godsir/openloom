//! Engine events emitted to the frontend via WebSocket.
//!
//! Consumers: loom-core (event bus), loom-server (WS notifications), loom-cli (TUI)

use serde::{Deserialize, Serialize};

use crate::mode::AgentState;

/// Events emitted from the engine to observers (frontend, logs).
///
/// Consumers: loom-core (event_bus broadcast), loom-server (event_to_notification), loom-cli (TUI)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineEvent {
    /// A cognition/trait was updated in the knowledge graph.
    CognitionUpdated {
        trait_name: String,
        old_value: String,
        new_value: String,
        confidence: f64,
    },
    /// Agent state transitioned.
    AgentStateChanged {
        old_state: AgentState,
        new_state: AgentState,
    },
    /// Streaming token delta from the LLM.
    StreamDelta { session_id: String, delta: String },
    /// Streaming response complete.
    StreamEnd {
        session_id: String,
        full_response: String,
    },
    /// Token usage report for this turn.
    TokenUsage {
        session_id: String,
        model: String,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_tokens: usize,
        latency_ms: u64,
        context_window: usize,
    },
    /// Engine-level error.
    Error {
        code: crate::jsonrpc::ErrorCode,
        message: String,
        subsystem: String,
    },
    /// Background heartbeat tick.
    HeartbeatTick {
        idle_minutes: u64,
        event_count: usize,
        suggested_action: Option<String>,
    },
    /// Permission required for a tool call.
    PermissionRequired {
        tool: String,
        params: serde_json::Value,
        risk_level: RiskLevel,
    },
    /// Tool execution started.
    ToolCallStarted {
        session_id: String,
        call_id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Tool execution completed.
    ToolCallEnded {
        session_id: String,
        call_id: String,
        name: String,
        success: bool,
        result_summary: String,
    },
    /// Tool execution progress update.
    ToolCallProgress {
        session_id: String,
        call_id: String,
        name: String,
        progress: Option<f64>,
        message: String,
    },
    /// Sub-agent state change notification.
    SubagentStateChanged {
        parent_id: String,
        child_id: String,
        child_name: String,
        status: String,
        result: Option<String>,
    },
    /// MCP server health changed.
    McpServerHealthChanged {
        server_name: String,
        status: String,
        error: Option<String>,
    },
    /// Session compaction was performed (infrastructure event).
    CompactionPerformed {
        session_id: String,
        tokens_before: usize,
        tokens_after: usize,
        savings_pct: f64,
        items_compacted: usize,
        strategies: Vec<String>,
        tool_outputs_truncated: usize,
        base64_elided: usize,
        loops_collapsed: usize,
        llm_summarization_used: bool,
        duration_ms: u64,
    },
}

/// Risk level for tool calls requiring user approval.
///
/// Consumers: loom-core (permission check), loom-security, loom-server (WS notification)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Forbidden,
}

/// A permission request sent from engine to UI for user approval.
///
/// Consumers: loom-core (agent loop permission), loom-server (WS notification)
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub description: String,
    pub risk_level: String,
}

/// Response from UI for a permission request.
///
/// Consumers: loom-core (agent loop), loom-server (dispatch)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResponse {
    pub approved: bool,
    /// If true, auto-approve this tool for the rest of the session.
    #[serde(default)]
    pub remember: bool,
}
