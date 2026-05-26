// SPDX-License-Identifier: MIT
//! Unified type system for openLoom v2.
//!
//! This is the SINGLE canonical type crate. All other crates depend on this one.
//! No other crate may define competing Message/Tool/Config types.
//!
//! # Anti-dumping-ground rules
//! 1. Three-consumer rule: types live here only if ≥3 crates import them
//! 2. No implementation logic (just data + trivial accessors)
//! 3. No utility functions (deep_merge, encoding, etc.)
//! 4. Module max 250 lines; split if exceeded
//! 5. Every pub type has a doc comment listing its consumers
//! 6. #[deprecated] over deletion for one version cycle

pub mod id;
pub mod role;
pub mod message;
pub mod tool;
pub mod mcp;
pub mod jsonrpc;
pub mod event;
pub mod config;
pub mod session;
pub mod mode;
pub mod router;
pub mod inference;
pub mod permission;
pub mod persona;

// Re-export all public types at crate root for ergonomic imports
pub use id::*;
pub use role::*;
pub use message::*;
pub use tool::*;
pub use mcp::*;
pub use jsonrpc::*;
pub use event::*;
pub use config::model_config::*;
pub use config::*;
pub use session::*;
pub use mode::*;
pub use router::*;
pub use inference::*;
pub use permission::*;
pub use persona::*;
