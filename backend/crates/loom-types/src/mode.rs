//! Agent operating mode and state types.
//!
//! Consumers: loom-core (agent loop dispatch), loom-server (RPC params), loom-context

use serde::{Deserialize, Serialize};

/// The agent's operating mode, controlling tool scope and behavior.
///
/// Consumers: loom-core (agent loop dispatch), loom-server, loom-context (system prompt suffix)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Chat,
    Plan,
    #[default]
    Code,
    Assistant,
}

/// Runtime agent state for UI display and lifecycle management.
///
/// Consumers: loom-core (AgentPool/agent loop), loom-server (agent.status RPC, WS events)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    #[default]
    Idle,
    Thinking,
    Acting,
    WaitingForSubagent,
    Interrupted,
    Errored,
    Completed,
    Killed,
}

impl AgentState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Completed | AgentState::Errored | AgentState::Killed)
    }

    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            AgentState::Thinking | AgentState::Acting | AgentState::WaitingForSubagent
        )
    }
}

/// Controls how deeply the model reasons before producing output.
///
/// Consumers: loom-core (agent loop), loom-server (session.thinking_level.set RPC)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    #[default]
    None,
    Low,
    Medium,
    High,
    Max,
}

/// Model routing preference for a session or agent.
///
/// Consumers: loom-core (router), loom-server (RPC params)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ModelPreference {
    #[default]
    Auto,
    Local,
    Cloud,
}
