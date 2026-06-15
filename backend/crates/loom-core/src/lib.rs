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
pub mod hooks;
pub mod orchestrator;
pub mod plan_prompts;
pub mod slash_router;
pub mod todo_context;
pub mod tool_context;
pub mod tool_registry;
pub mod vision;

pub use agent::{Agent, AgentConfig, AgentStatus};
pub use agent_loop::{AgentLoopConfig, TurnResult, run_agent_turn, run_agent_turn_streaming};
pub use agent_pool::AgentPool;
pub use event_bus::{AgentEvent, EventBus};
pub use orchestrator::{
    MemoryStore, Orchestrator, PipelineScheduler, adapt_behavior,
};
pub use loom_types::{
    BehaviorProfile, ForgettingReport, MemoryHealth, PipelineStage, QualityEvaluation,
};
pub use slash_router::{SlashIntercept, SlashRouter};
pub use tool_registry::{
    AgentTool, SpawnAgentTool, SpawnContext, ToolProvenance, ToolRegistry, ToolResult,
};
