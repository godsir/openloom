//! WebSocket handler — JSON-RPC 2.0 over WebSocket with bidirectional messaging.
//! Parses incoming requests, routes to dispatch, sends responses and notifications.

use std::sync::Arc;

use crate::AppState;
use crate::dispatch;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use loom_core::AgentEvent;
use loom_types::{JsonRpcRequest, JsonRpcResponse};
use tokio::sync::broadcast;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    tracing::info!("WebSocket connected");

    let mut event_rx = state.orchestrator.event_bus().subscribe();

    loop {
        tokio::select! {
            // Incoming client messages
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<JsonRpcRequest>(&text) {
                            Ok(req) => {
                                let response = dispatch::dispatch_method(&state, &req).await;
                                let resp = JsonRpcResponse {
                                    jsonrpc: "2.0".into(),
                                    result: response.as_ref().ok().cloned(),
                                    error: response.as_ref().err().cloned(),
                                    id: req.id,
                                };
                                if let Ok(json) = serde_json::to_string(&resp) {
                                    let _ = socket.send(Message::Text(json)).await;
                                }
                            }
                            Err(_) => {
                                let _ = socket.send(Message::Text(
                                    r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":0}"#.into()
                                )).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::debug!("WS error: {}", e);
                        break;
                    }
                    _ => {} // ignore binary/ping/pong
                }
            }
            // Outgoing server events: forward AgentEvents as WS notifications
            event = event_rx.recv() => {
                match event {
                    Ok(ref e) => {
                        let method = agent_event_method(e);
                        if let Ok(json) = serde_json::to_string(&serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": method,
                            "params": e,
                        })) {
                            let _ = socket.send(Message::Text(json)).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "WS event lag");
                        // Resubscribe to reset the receiver
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
    }
}
