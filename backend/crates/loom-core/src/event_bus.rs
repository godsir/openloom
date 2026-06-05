//! Event types produced by agents and consumed by observers (server, CLI, logs).

use loom_types::AgentId;
use serde::{Deserialize, Serialize};

use crate::agent::AgentStatus;

/// Events emitted by agents during their lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEvent {
    /// Agent state transitioned.
    StateChanged {
        agent_id: AgentId,
        old_status: AgentStatus,
        new_status: AgentStatus,
    },
    /// A child/sub-agent was spawned.
    SubagentSpawned {
        parent_id: AgentId,
        child_id: AgentId,
        child_name: String,
    },
    /// A sub-agent completed its task.
    SubagentCompleted {
        parent_id: AgentId,
        child_id: AgentId,
        result: String,
    },
    /// A sub-agent errored.
    SubagentErrored {
        parent_id: AgentId,
        child_id: AgentId,
        error: String,
    },
    /// Tool execution started.
    ToolStarted {
        agent_id: AgentId,
        call_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    /// Tool execution completed.
    ToolCompleted {
        agent_id: AgentId,
        call_id: String,
        tool_name: String,
        success: bool,
        result: Option<String>,
        structured_content: Option<serde_json::Value>,
    },
    /// LLM token streaming delta.
    StreamDelta {
        agent_id: AgentId,
        session_id: String,
        delta: String,
    },
    /// Streaming response complete.
    StreamEnd {
        agent_id: AgentId,
        session_id: String,
        full_response: String,
    },
    /// Token usage report.
    TokenUsage {
        agent_id: AgentId,
        session_id: String,
        model: String,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_tokens: usize,
        cache_read_tokens: usize,
        cache_write_tokens: usize,
        latency_ms: u64,
        /// Model context window in tokens. 0 if unknown.
        context_window: usize,
    },
    /// Permission request for "ask" mode — frontend should show confirmation dialog.
    PermissionRequest {
        agent_id: AgentId,
        session_id: String,
        call_id: String,
        tool_name: String,
        args: serde_json::Value,
        risk: String,
    },
    /// Memory (KG/cognitions) was updated for a session — frontend can refresh KG display.
    MemoryUpdated { session_id: String },
}

/// A lightweight event bus using tokio broadcast.
/// Multiple observers can subscribe to agent events concurrently.
#[derive(Debug, Clone)]
pub struct EventBus {
    tx: tokio::sync::broadcast::Sender<AgentEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(capacity);
        Self { tx }
    }

    /// Publish an event to all subscribers.
    pub fn publish(&self, event: AgentEvent) {
        let _ = self.tx.send(event);
    }

    /// Subscribe to receive events.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AgentEvent> {
        self.tx.subscribe()
    }
}
