//! WebSocket handler — JSON-RPC 2.0 over WebSocket with bidirectional messaging.
//! Parses incoming requests, routes to dispatch, sends responses and notifications.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use crate::AppState;
use crate::dispatch;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use loom_core::AgentEvent;
use loom_types::{JsonRpcRequest, JsonRpcResponse};
use tokio::sync::broadcast;

/// Per-connection ring buffer that retains events for replay after disconnect.
pub struct ConnectionEventLog {
    events: VecDeque<(u64, String)>, // (seq, JSON notification)
    capacity: usize,
    pub next_seq: u64,
    disconnected_at: Option<Instant>,
}

impl ConnectionEventLog {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
            next_seq: 1,
            disconnected_at: None,
        }
    }

    /// Push an event, assign a seq, trim if over capacity.
    pub fn push(&mut self, json: String) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.events.push_back((seq, json));
        while self.events.len() > self.capacity {
            self.events.pop_front();
        }
        seq
    }

    /// Return all events with seq > after_seq, in order.
    pub fn replay_from(&self, after_seq: u64) -> Vec<&(u64, String)> {
        self.events.iter().filter(|(s, _)| *s > after_seq).collect()
    }

    pub fn mark_disconnected(&mut self) {
        self.disconnected_at = Some(Instant::now());
    }

    #[allow(dead_code)]
    pub fn is_stale(&self, ttl: std::time::Duration) -> bool {
        self.disconnected_at
            .map(|t| t.elapsed() > ttl)
            .unwrap_or(false)
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // Reject new WebSocket connections during shutdown
    if state.shutdown_token.is_cancelled() {
        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let from_seq: Option<u64> = params.get("seq").and_then(|s| s.parse().ok());
    ws.on_upgrade(move |socket| handle_socket(socket, state, from_seq))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>, from_seq: Option<u64>) {
    tracing::info!("WebSocket connected");

    let mut event_rx = state.orchestrator.event_bus().subscribe();
    let event_log = state.event_log.clone();
    let (mut ws_tx, mut ws_rx) = socket.split();

    // If reconnecting with a seq, replay missed events first
    {
        let log = event_log.lock().await;
        if let Some(seq) = from_seq {
            let replay = log.replay_from(seq);
            if !replay.is_empty() {
                for (_s, json) in &replay {
                    let _ = ws_tx.send(Message::Text(json.clone())).await;
                }
            }
            // Send replay_done marker
            let from = seq.saturating_add(1);
            let to = log.next_seq.saturating_sub(1);
            let done = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "ws.replay_done",
                "params": { "from_seq": from, "to_seq": to }
            });
            let _ = ws_tx.send(Message::Text(done.to_string())).await;
        }
    }

    // Channel for sending responses back through the WebSocket
    let (resp_tx, mut resp_rx) = tokio::sync::mpsc::channel::<String>(64);

    loop {
        tokio::select! {
            // Incoming client messages
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<JsonRpcRequest>(&text) {
                            Ok(req) => {
                                if req.method == "chat.send" {
                                    // Long-running: spawn in background so other RPCs aren't blocked
                                    let st = state.clone();
                                    let tx = resp_tx.clone();
                                    tokio::spawn(async move {
                                        let response = dispatch::dispatch_method(&st, &req).await;
                                        let resp = JsonRpcResponse {
                                            jsonrpc: "2.0".into(),
                                            result: response.as_ref().ok().cloned(),
                                            error: response.as_ref().err().cloned(),
                                            id: req.id,
                                        };
                                        if let Ok(json) = serde_json::to_string(&resp) {
                                            let _ = tx.send(json).await;
                                        }
                                    });
                                } else {
                                    // Short RPCs: dispatch inline
                                    let response = dispatch::dispatch_method(&state, &req).await;
                                    let resp = JsonRpcResponse {
                                        jsonrpc: "2.0".into(),
                                        result: response.as_ref().ok().cloned(),
                                        error: response.as_ref().err().cloned(),
                                        id: req.id,
                                    };
                                    if let Ok(json) = serde_json::to_string(&resp) {
                                        let _ = ws_tx.send(Message::Text(json)).await;
                                    }
                                }
                            }
                            Err(_) => {
                                let _ = ws_tx.send(Message::Text(
                                    r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":0}"#.into()
                                )).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        event_log.lock().await.mark_disconnected();
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::debug!("WS error: {}", e);
                        event_log.lock().await.mark_disconnected();
                        break;
                    }
                    _ => {} // ignore binary/ping/pong
                }
            }
            // Responses from background tasks (chat.send)
            Some(json) = resp_rx.recv() => {
                let _ = ws_tx.send(Message::Text(json)).await;
            }
            // Outgoing server events: forward AgentEvents as WS notifications
            event = event_rx.recv() => {
                match event {
                    Ok(ref e) => {
                        let method = agent_event_method(e);
                        let params = agent_event_params(e);
                        let mut log = event_log.lock().await;
                        let seq = log.next_seq;
                        if let Ok(json) = serde_json::to_string(&serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": method,
                            "params": params,
                            "seq": seq,
                        })) {
                            log.push(json.clone());
                            drop(log);
                            let _ = ws_tx.send(Message::Text(json)).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "WS event lag");
                        event_rx = state.orchestrator.event_bus().subscribe();
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    tracing::info!("WebSocket disconnected");
}

fn agent_event_method(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::StateChanged { .. } => "agent.state_changed",
        AgentEvent::SubagentSpawned { .. } => "agent.subagent_spawned",
        AgentEvent::SubagentCompleted { .. } => "agent.subagent_completed",
        AgentEvent::SubagentErrored { .. } => "agent.subagent_errored",
        AgentEvent::ToolStarted { .. } => "tool.started",
        AgentEvent::ToolCompleted { .. } => "tool.completed",
        AgentEvent::StreamDelta { .. } => "chat.stream_delta",
        AgentEvent::StreamEnd { .. } => "chat.stream_end",
        AgentEvent::TokenUsage { .. } => "chat.token_usage",
        AgentEvent::PermissionRequest { .. } => "tool.permission_request",
        AgentEvent::MemoryUpdated { .. } => "memory.updated",
        AgentEvent::PlanCreated { .. } => "plan.created",
        AgentEvent::PlanUpdated { .. } => "plan.updated",
        AgentEvent::GoalSet { .. } => "goal.set",
        AgentEvent::TodoStatusChanged { .. } => "todo.status_changed",
        AgentEvent::TodosReplaced { .. } => "todo.list_replaced",
        AgentEvent::CronJobTriggered { .. } => "cron.job_triggered",
        AgentEvent::CronJobCompleted { .. } => "cron.job_completed",
        AgentEvent::CronJobFailed { .. } => "cron.job_failed",
        AgentEvent::CronJobChanged { .. } => "cron.job_changed",
        AgentEvent::ProcessOutput { .. } => "process.output",
        AgentEvent::ProcessExited { .. } => "process.exited",
        AgentEvent::MonitorStarted { .. } => "monitor.started",
        AgentEvent::MonitorOutput { .. } => "monitor.output",
        AgentEvent::MonitorExited { .. } => "monitor.exited",
        AgentEvent::MonitorError { .. } => "monitor.error",
    }
}

fn agent_event_params(event: &AgentEvent) -> serde_json::Value {
    use serde_json::json;
    match event {
        AgentEvent::StreamDelta {
            agent_id: _,
            session_id,
            delta,
        } => {
            json!({ "session_id": session_id, "delta": delta })
        }
        AgentEvent::StreamEnd {
            agent_id: _,
            session_id,
            full_response: _,
        } => {
            json!({ "session_id": session_id })
        }
        AgentEvent::ToolStarted {
            agent_id: _,
            call_id,
            tool_name,
            args,
        } => {
            json!({ "id": call_id, "name": tool_name, "args": args })
        }
        AgentEvent::ToolCompleted {
            agent_id: _,
            call_id,
            tool_name,
            success,
            result,
            structured_content,
        } => {
            json!({ "id": call_id, "name": tool_name, "success": success, "result": result, "structured_content": structured_content })
        }
        AgentEvent::TokenUsage {
            agent_id: _,
            session_id,
            model,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            cache_read_tokens,
            cache_write_tokens,
            latency_ms,
            context_window,
        } => {
            json!({
                "session_id": session_id,
                "model": model,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "cached_tokens": cached_tokens,
                "cache_read_tokens": cache_read_tokens,
                "cache_write_tokens": cache_write_tokens,
                "latency_ms": latency_ms,
                "context_window": context_window,
            })
        }
        AgentEvent::PermissionRequest {
            agent_id: _,
            session_id,
            call_id,
            tool_name,
            args,
            risk,
        } => {
            json!({
                "session_id": session_id,
                "call_id": call_id,
                "tool_name": tool_name,
                "args": args,
                "risk": risk,
            })
        }
        AgentEvent::PlanCreated { plan_id, title } => {
            json!({ "plan_id": plan_id, "title": title })
        }
        AgentEvent::PlanUpdated { plan_id } => {
            json!({ "plan_id": plan_id })
        }
        AgentEvent::GoalSet { session_id, description } => {
            json!({ "session_id": session_id, "description": description })
        }
        AgentEvent::TodoStatusChanged { session_id, todo_id, status } => {
            json!({ "session_id": session_id, "todo_id": todo_id, "status": status })
        }
        AgentEvent::TodosReplaced { session_id, todos } => {
            json!({ "session_id": session_id, "todos": todos })
        }
        AgentEvent::CronJobTriggered { job_id, job_name, run_id } => {
            json!({ "job_id": job_id, "job_name": job_name, "run_id": run_id })
        }
        AgentEvent::CronJobCompleted { job_id, job_name, run_id, response } => {
            json!({ "job_id": job_id, "job_name": job_name, "run_id": run_id, "response": response })
        }
        AgentEvent::CronJobFailed { job_id, job_name, run_id, error } => {
            json!({ "job_id": job_id, "job_name": job_name, "run_id": run_id, "error": error })
        }
        AgentEvent::CronJobChanged { job_id, action } => {
            json!({ "job_id": job_id, "action": action })
        }
        AgentEvent::ProcessOutput { pid, data, stream } => {
            json!({ "pid": pid, "data": data, "stream": stream })
        }
        AgentEvent::ProcessExited { pid, exit_code } => {
            json!({ "pid": pid, "exit_code": exit_code })
        }
        AgentEvent::MonitorStarted {
            monitor_id,
            name,
            source,
            persistent,
            started_at_ms,
        } => {
            json!({
                "monitor_id": monitor_id,
                "name": name,
                "source": source,
                "persistent": persistent,
                "started_at_ms": started_at_ms,
            })
        }
        AgentEvent::MonitorOutput {
            monitor_id,
            data,
            stream,
        } => {
            json!({
                "monitor_id": monitor_id,
                "data": data,
                "stream": stream,
            })
        }
        AgentEvent::MonitorExited {
            monitor_id,
            exit_code,
        } => {
            json!({
                "monitor_id": monitor_id,
                "exit_code": exit_code,
            })
        }
        AgentEvent::MonitorError {
            monitor_id,
            error,
        } => {
            json!({
                "monitor_id": monitor_id,
                "error": error,
            })
        }
        _ => json!({}),
    }
}
