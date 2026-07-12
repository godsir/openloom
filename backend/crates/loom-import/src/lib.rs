//! External conversation import — parsers + mappers (no SQL).
//!
//! Consumers: loom-server (dispatch::claude_import)

mod claude;

pub use claude::{ConversationSummary, scan};
