//! Session metadata types.
//!
//! Consumers: loom-core (session management), loom-server (session.* RPC), loom-memory

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Lightweight session metadata for listing and routing.
///
/// Consumers: loom-core (SessionStore), loom-server (session.list/switch/create)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub message_count: usize,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pinned_at: Option<String>,
    #[serde(default)]
    pub archived_at: Option<String>,
}
