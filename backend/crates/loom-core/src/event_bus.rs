//! Event types produced by agents and consumed by observers (server, CLI, logs).

use loom_types::AgentId;
use serde::{Deserialize, Serialize};

use crate::agent::AgentStatus;

/// 一条插话队列项，带唯一 ID，前端据此追踪每项的消费状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteeringItem {
    pub id: String,
    pub text: String,
}

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
        session_id: String,
    },
    /// Tool execution completed.
    ToolCompleted {
        agent_id: AgentId,
        call_id: String,
        tool_name: String,
        success: bool,
        result: Option<String>,
        structured_content: Option<serde_json::Value>,
        session_id: String,
    },
    /// Tool produced a line of output (streaming, e.g. shell stdout/stderr).
    ToolOutput {
        agent_id: AgentId,
        call_id: String,
        tool_name: String,
        line: String,
        stream: String,
        session_id: String,
    },
    /// LLM token streaming delta.
    StreamDelta {
        agent_id: AgentId,
        session_id: String,
        delta: String,
        /** Sub-agent name when this delta is from a team member */
        child_name: Option<String>,
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
    GoalSet {
        session_id: String,
        description: String,
    },
    /// A todo item status changed.
    TodoStatusChanged {
        session_id: String,
        todo_id: String,
        status: String,
    },
    /// The entire todo list was replaced (todo_write called by AI).
    TodosReplaced {
        session_id: String,
        todos: serde_json::Value,
    },
    /// AI wants to push a desktop notification to the user.
    PushNotification {
        session_id: String,
        title: String,
        body: String,
    },
    /// AI reports structured code-review findings.
    ReviewFindings {
        session_id: String,
        findings: serde_json::Value,
    },
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
    CronJobChanged { job_id: String, action: String },
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
    /// 团队启动
    TeamStarted {
        team_id: String,
        team_name: String,
        captain_id: AgentId,
        member_ids: Vec<AgentId>,
    },
    /// 团队成员完成一轮
    TeamMemberDone {
        team_id: String,
        member_id: AgentId,
        member_name: String,
        round: usize,
        /// Token usage for this member's turn.
        prompt_tokens: usize,
        completion_tokens: usize,
    },
    /// 团队一轮完成
    TeamRoundComplete { team_id: String, round: usize },
    /// 团队执行完毕
    TeamCompleted {
        team_id: String,
        session_id: String,
        summary: String,
    },
    /// 团队成员流式输出（实时）
    TeamMemberDelta {
        team_id: String,
        member_name: String,
        delta: String,
        session_id: String,
    },
    /// 团队成员开始执行
    TeamMemberStarted {
        team_id: String,
        member_name: String,
        session_id: String,
    },
    /// 用户插话已加入 steering queue，等待 agent 下一轮迭代消费
    SteeringQueued {
        session_id: String,
        /** 当前队列中的插话数量 */
        pending_count: usize,
        /** 本次加入的插话项 */
        item: SteeringItem,
    },
    /// Agent 已消费 steering queue 中的插话项
    SteeringConsumed {
        session_id: String,
        /** 队列中剩余的插话数量 */
        remaining_count: usize,
        /** 本次被消费的插话项 */
        items: Vec<SteeringItem>,
    },
    /// 偏好设置被 update_config 工具修改，前端应实时应用
    PreferencesChanged {
        /** 变更的 key-value 对 */
        updates: std::collections::HashMap<String, serde_json::Value>,
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
