//! Agent struct, state machine, and configuration.
//!
//! Each Agent is an independent entity with its own id, persona, model binding,
//! tool set, conversation history, and lifecycle state.

use chrono::{DateTime, Utc};
use loom_types::{AgentConfig as AgentConfigType, AgentState, SessionId};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tokio::task::JoinHandle;

use crate::event_bus::AgentEvent;

// Re-export the config type from loom-types
pub use loom_types::AgentConfig;

/// The 9-state agent lifecycle machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Thinking,
    Acting,
    WaitingForSubagent,
    Interrupted,
    Errored { message: String },
    Completed,
    Killed,
}

impl AgentStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentStatus::Completed | AgentStatus::Errored { .. } | AgentStatus::Killed)
    }

    pub fn is_busy(&self) -> bool {
        matches!(self, AgentStatus::Thinking | AgentStatus::Acting | AgentStatus::WaitingForSubagent)
    }

    pub fn to_loom_state(&self) -> AgentState {
        match self {
            AgentStatus::Idle => AgentState::Idle,
            AgentStatus::Thinking => AgentState::Thinking,
            AgentStatus::Acting => AgentState::Acting,
            AgentStatus::WaitingForSubagent => AgentState::WaitingForSubagent,
            AgentStatus::Interrupted => AgentState::Interrupted,
            AgentStatus::Errored { .. } => AgentState::Errored,
            AgentStatus::Completed => AgentState::Completed,
            AgentStatus::Killed => AgentState::Killed,
        }
    }
}

/// A running agent instance.
pub struct Agent {
    pub id: loom_types::AgentId,
    pub config: AgentConfigType,

    // Tree
    pub parent_id: Option<loom_types::AgentId>,
    pub children: Vec<loom_types::AgentId>,

    // State
    pub status: AgentStatus,
    pub status_message: Option<String>,

    // Session binding
    pub session_id: SessionId,
    pub history: Vec<loom_types::Message>,

    // Execution
    pub iteration: usize,
    pub started_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub error: Option<String>,

    // Result
    pub result: Option<String>,

    // Tokio
    pub handle: Option<JoinHandle<AgentResult>>,
    pub cancel_token: CancellationToken,
}

/// Result from a completed agent run.
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub agent_id: loom_types::AgentId,
    pub status: AgentStatus,
    pub response: Option<String>,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
}

impl Agent {
    /// Create a new agent from config, bound to a session.
    pub fn new(config: AgentConfigType, session_id: SessionId, parent_id: Option<loom_types::AgentId>) -> Self {
        let id = loom_types::AgentId::new();
        let now = Utc::now();
        Self {
            id,
            config,
            parent_id,
            children: Vec::new(),
            status: AgentStatus::Idle,
            status_message: None,
            session_id,
            history: Vec::new(),
            iteration: 0,
            started_at: now,
            last_active_at: now,
            error: None,
            result: None,
            handle: None,
            cancel_token: CancellationToken::new(),
        }
    }

    /// Transition to a new status, emitting the change.
    pub fn transition(&mut self, new_status: AgentStatus, msg: Option<String>) -> Option<AgentEvent> {
        let old_status = self.status.clone();
        if old_status == new_status {
            return None;
        }
        self.status = new_status.clone();
        self.status_message = msg;
        self.last_active_at = Utc::now();
        Some(AgentEvent::StateChanged {
            agent_id: self.id.clone(),
            old_status,
            new_status,
        })
    }

    /// Add a child agent ID to track.
    pub fn add_child(&mut self, child_id: loom_types::AgentId) {
        self.children.push(child_id);
    }

    /// Remove a completed child agent.
    pub fn remove_child(&mut self, child_id: &loom_types::AgentId) {
        self.children.retain(|c| c != child_id);
    }

    /// Append a message to conversation history.
    pub fn push_history(&mut self, msg: loom_types::Message) {
        self.history.push(msg);
    }
}
