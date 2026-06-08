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

pub mod config;
pub mod event;
pub mod id;
pub mod inference;
pub mod jsonrpc;
pub mod kg;
pub mod mcp;
pub mod memory;
pub mod message;
pub mod mode;
pub mod permission;
pub mod persona;
pub mod plan;
pub mod role;
pub mod router;
pub mod session;
pub mod tool;

// Re-export all public types at crate root for ergonomic imports
pub use config::compaction::*;
pub use config::model_config::*;
pub use config::*;
pub use event::*;
pub use id::*;
pub use inference::*;
pub use jsonrpc::*;
pub use kg::*;
pub use mcp::*;
pub use memory::*;
pub use message::*;
pub use mode::*;
pub use permission::*;
pub use persona::*;
pub use plan::*;
pub use role::*;
pub use router::*;
pub use session::*;
pub use tool::*;
