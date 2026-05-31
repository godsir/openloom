//! WebSocket handler — JSON-RPC 2.0 over WebSocket with bidirectional messaging.
//! Parses incoming requests, routes to dispatch, sends responses and notifications.

use std::sync::Arc;

use crate::AppState;
use crate::dispatch;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use loom_core::AgentEvent;
use loom_types::{JsonRpcRequest, JsonRpcResponse};
use tokio::sync::broadcast;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    tracing::info!("WebSocket connected");

    let mut event_rx = state.orchestrator.event_bus().subscribe();
    let (mut ws_tx, mut ws_rx) = socket.split();

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
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::debug!("WS error: {}", e);
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
                        if let Ok(json) = serde_json::to_string(&serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": method,
                            "params": params,
                        })) {
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
            latency_ms,
            context_window,
        } => {
            json!({
                "session_id": session_id,
                "model": model,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "cached_tokens": cached_tokens,
                "latency_ms": latency_ms,
                "context_window": context_window,
            })
        }
        _ => json!({}),
    }
}
