//! Strongly-typed identifiers for agents and sessions.
//!
//! Consumers: loom-core, loom-server, loom-memory, loom-cli

use serde::{Deserialize, Serialize};

/// Unique agent identifier.
///
/// Consumers: loom-core (AgentPool), loom-server (dispatch), loom-cli (commands)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new() -> Self {
        Self(format!(
            "agent-{}",
            uuid::Uuid::now_v7().to_string().replace('-', "")
        ))
    }

    pub const DEFAULT: Self = AgentId(String::new());

    pub fn is_default(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Unique session identifier.
///
/// Consumers: loom-core (agent loop), loom-server (dispatch), loom-memory (store)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}
