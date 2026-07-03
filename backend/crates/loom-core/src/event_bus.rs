//! Event types produced by agents and consumed by observers (server, CLI, logs).

use loom_types::AgentId;
use serde::{Deserialize, Serialize};

use crate::agent::AgentStatus;

/// Events emitted by agents during their lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEvent {
    /// Agent state transitioned.
    StateChanged {
        agent_id: AgentId,
        old_status: AgentStatus,
        new_status: AgentStatus,
    },
    /// A child/sub-agent was spawned.
    SubagentSpawned {
        parent_id: AgentId,
        child_id: AgentId,
        child_name: String,
    },
    /// A sub-agent completed its task.
    SubagentCompleted {
        parent_id: AgentId,
        child_id: AgentId,
        result: String,
    },
    /// A sub-agent errored.
    SubagentErrored {
        parent_id: AgentId,
        child_id: AgentId,
        error: String,
    },
    /// Tool execution started.
    ToolStarted {
        agent_id: AgentId,
        call_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    /// Tool execution completed.
    ToolCompleted {
        agent_id: AgentId,
        call_id: String,
        tool_name: String,
        success: bool,
        result: Option<String>,
        structured_content: Option<serde_json::Value>,
    },
    /// LLM token streaming delta.
    StreamDelta {
        agent_id: AgentId,
        session_id: String,
        delta: String,
    },
    /// Streaming response complete.
    StreamEnd {
        agent_id: AgentId,
        session_id: String,
        full_response: String,
    },
    /// Token usage report.
    TokenUsage {
        agent_id: AgentId,
        session_id: String,
        model: String,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_tokens: usize,
        cache_read_tokens: usize,
        cache_write_tokens: usize,
        latency_ms: u64,
        /// Model context window in tokens. 0 if unknown.
        context_window: usize,
    },
    /// Permission request for "ask" mode — frontend should show confirmation dialog.
    PermissionRequest {
        agent_id: AgentId,
        session_id: String,
        call_id: String,
        tool_name: String,
        args: serde_json::Value,
        risk: String,
    },
    /// Memory (KG/cognitions) was updated for a session — frontend can refresh KG display.
    MemoryUpdated { session_id: String },
    /// A new plan was created.
    PlanCreated { plan_id: String, title: String },
    /// A plan was updated.
    PlanUpdated { plan_id: String },
    /// A session goal was set.
    GoalSet { session_id: String, description: String },
    /// A todo item status changed.
    TodoStatusChanged { session_id: String, todo_id: String, status: String },
    /// The entire todo list was replaced (todo_write called by AI).
    TodosReplaced { session_id: String, todos: serde_json::Value },
    /// A cron job was triggered (started executing).
    CronJobTriggered {
        job_id: String,
        job_name: String,
        run_id: String,
    },
    /// A cron job completed successfully.
    CronJobCompleted {
        job_id: String,
        job_name: String,
        run_id: String,
        response: String,
    },
    /// A cron job failed.
    CronJobFailed {
        job_id: String,
        job_name: String,
        run_id: String,
        error: String,
    },
    /// A cron job was created, updated, or deleted.
    CronJobChanged {
        job_id: String,
        action: String,
    },
    /// Background process emitted a line on stdout or stderr.
    ProcessOutput {
        pid: String,
        data: String,
        stream: String,
        session_id: String,
    },
    /// Background process exited.
    ProcessExited {
        pid: String,
        exit_code: i32,
        session_id: String,
    },
    /// Monitor 已启动
    MonitorStarted {
        monitor_id: String,
        name: String,
        source: String,
        persistent: bool,
        started_at_ms: i64,
        session_id: String,
    },
    /// Monitor 输出行（200ms 批处理合并后的结果）
    MonitorOutput {
        monitor_id: String,
        data: String,
        stream: String,
        session_id: String,
    },
    /// Monitor 已退出（进程结束 / WS 断开）
    MonitorExited {
        monitor_id: String,
        exit_code: i32,
        session_id: String,
    },
    /// Monitor 错误（启动失败、限流停止等）
    MonitorError {
        monitor_id: String,
        error: String,
        session_id: String,
    },
}

/// A lightweight event bus using tokio broadcast.
/// Multiple observers can subscribe to agent events concurrently.
#[derive(Debug, Clone)]
pub struct EventBus {
    tx: tokio::sync::broadcast::Sender<AgentEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(capacity);
        Self { tx }
    }

    /// Publish an event to all subscribers.
    pub fn publish(&self, event: AgentEvent) {
        let _ = self.tx.send(event);
    }

    /// Get a clone of the underlying broadcast sender.
    pub fn sender(&self) -> tokio::sync::broadcast::Sender<AgentEvent> {
        self.tx.clone()
    }

    /// Subscribe to receive events.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AgentEvent> {
        self.tx.subscribe()
    }
}
