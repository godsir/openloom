// SPDX-License-Identifier: Apache-2.0
//! Loom Core — Agent orchestrator for openLoom v2.
//!
//! This crate provides the Agent lifecycle, AgentPool for concurrent execution,
//! tool dispatch (builtin + MCP), event bus, and the top-level orchestrator.

pub mod agent;
pub mod agent_loop;
pub mod agent_pool;
pub mod builtin_tools;
pub mod context_probe;
pub mod entity_cron_tools;
pub mod entity_mcp_tools;
pub mod entity_skills_tools;
pub mod entity_tools;
pub mod event_bus;
pub mod monitor_manager;
pub mod orchestrator;
pub mod plan_prompts;
pub mod process_manager;
pub mod slash_router;
pub mod slim;
pub mod team_orchestrator;
pub mod todo_context;
pub mod tool_context;
pub mod tool_registry;
pub mod vision;

pub use agent::{Agent, AgentConfig, AgentStatus};
pub use agent_loop::{AgentLoopConfig, TurnResult, run_agent_turn, run_agent_turn_streaming};
pub use agent_pool::AgentPool;
pub use event_bus::{AgentEvent, EventBus};
pub use loom_types::{
    BehaviorProfile, ForgettingReport, MemoryHealth, PipelineStage, QualityEvaluation,
};
pub use monitor_manager::{MonitorInfo, MonitorManager, MonitorWsConfig};
pub use orchestrator::{MemoryStore, Orchestrator, PipelineScheduler, adapt_behavior};
pub use process_manager::{ProcessInfo, ProcessManager};
pub use slash_router::{SlashIntercept, SlashRouter};
pub use tool_registry::{
    AgentTool, SpawnAgentTool, SpawnAgentsTool, SpawnContext, ToolProvenance, ToolRegistry,
    ToolResult,
};
