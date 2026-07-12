//! Types for importing external conversations (e.g. Claude Code JSONL).
//!
//! Consumers: loom-import (parser), loom-core (MemoryStore trait), loom-cli (impl)

use chrono::{DateTime, Utc};

use crate::Message;

/// A fully-parsed conversation ready to persist into openLoom's session store.
///
/// `id` is the source conversation's UUID — it becomes `sessions.id`,
/// giving imports natural idempotency (INSERT OR IGNORE).
#[derive(Debug, Clone)]
pub struct ImportPayload {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub title: Option<String>,
    pub workspace_path: Option<String>,
    /// Mapped messages, in order. `usage` is set on assistant turns.
    pub messages: Vec<Message>,
}

/// Result of persisting an [`ImportPayload`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    Created,
    AlreadyExists,
    Replaced,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_variants_are_distinct() {
        assert_ne!(ImportOutcome::Created, ImportOutcome::AlreadyExists);
        assert_ne!(ImportOutcome::AlreadyExists, ImportOutcome::Replaced);
    }

    #[test]
    fn payload_holds_messages() {
        let p = ImportPayload {
            id: "abc".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            title: Some("t".into()),
            workspace_path: Some("C:/x".into()),
            messages: vec![Message::user("hi")],
        };
        assert_eq!(p.messages.len(), 1);
        assert_eq!(p.id, "abc");
    }
}
