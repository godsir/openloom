// SPDX-License-Identifier: Apache-2.0
//! Loom Core — Agent orchestrator for openLoom v2.
//!
//! This crate provides the Agent lifecycle, AgentPool for concurrent execution,
//! tool dispatch (builtin + MCP), event bus, and the top-level orchestrator.

pub mod agent;
pub mod agent_loop;
pub mod agent_pool;
pub mod builtin_tools;
pub mod event_bus;
pub mod orchestrator;
pub mod tool_registry;

pub use agent::{Agent, AgentConfig, AgentStatus};
pub use agent_loop::{run_agent_turn, run_agent_turn_streaming, AgentLoopConfig, TurnResult};
pub use agent_pool::AgentPool;
pub use event_bus::{AgentEvent, EventBus};
pub use orchestrator::{MemoryStore, Orchestrator};
pub use tool_registry::{AgentTool, SpawnAgentTool, SpawnContext, ToolRegistry, ToolResult, ToolProvenance};
