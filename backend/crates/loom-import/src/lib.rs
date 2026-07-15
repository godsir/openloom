//! External conversation import — parsers + mappers (no SQL).
//!
//! Consumers: loom-server (dispatch::claude_import / codex_import / openclaw_import)

mod claude;
pub mod codex;
pub mod openclaw;

pub use claude::{ConversationSummary, build_payload, scan};
