use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use openloom_engine::Engine;
use openloom_models::*;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::dispatch;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(engine): State<Arc<Engine>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, engine))
}

async fn handle_ws(mut socket: WebSocket, engine: Arc<Engine>) {
    let mut event_rx = engine.subscribe();

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&text) {
                            let result = dispatch::dispatch_method(&engine, &req.method, req.params.clone()).await;
                            let resp = match result {
                                Ok(value) => JsonRpcResponse {
                                    jsonrpc: "2.0".into(), result: Some(value), error: None, id: req.id,
                                },
                                Err(err) => JsonRpcResponse {
                                    jsonrpc: "2.0".into(), result: None, error: Some(err), id: req.id,
                                },
                            };
                            if let Ok(json) = serde_json::to_string(&resp) {
                                let _ = socket.send(Message::Text(json)).await;
                            }
                        } else {
                            let err = JsonRpcResponse {
                                jsonrpc: "2.0".into(), result: None,
                                error: Some(JsonRpcError {
                                    code: ErrorCode::ParseError, message: "invalid JSON-RPC".into(), data: None,
                                }),
                                id: 0,
                            };
                            if let Ok(json) = serde_json::to_string(&err) {
                                let _ = socket.send(Message::Text(json)).await;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_))) => {
                        let _ = socket.send(Message::Pong(vec![])).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            result = event_rx.recv() => {
                match result {
                    Ok(event) => {
                        let notification = event_to_notification(&event);
                        if let Ok(json) = serde_json::to_string(&notification) {
                            let _ = socket.send(Message::Text(json)).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "WebSocket event_rx lagging, skipped events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

fn event_to_notification(event: &EngineEvent) -> serde_json::Value {
    match event {
        EngineEvent::CognitionUpdated {
            trait_name,
            new_value,
            confidence,
            ..
        } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "cognition.updated",
                "params": { "trait": trait_name, "new_value": new_value, "confidence": confidence }
            })
        }
        EngineEvent::AgentStateChanged {
            old_state,
            new_state,
        } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "agent.state_changed",
                "params": { "old_state": old_state, "new_state": new_state }
            })
        }
        EngineEvent::TokenUsage {
            session_id,
            model,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            latency_ms,
        } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "token.usage",
                "params": {
                    "session_id": session_id, "model": model,
                    "prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens,
                    "cached_tokens": cached_tokens, "latency_ms": latency_ms
                }
            })
        }
        EngineEvent::Error {
            code,
            message,
            subsystem,
        } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "error",
                "params": { "code": code, "message": message, "subsystem": subsystem }
            })
        }
        EngineEvent::HeartbeatTick {
            idle_minutes,
            event_count,
            suggested_action,
        } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "heartbeat.tick",
                "params": {
                    "idle_minutes": idle_minutes,
                    "event_count": event_count,
                    "suggested_action": suggested_action
                }
            })
        }
        EngineEvent::PermissionRequired {
            tool,
            params,
            risk_level,
        } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "permission.required",
                "params": {
                    "tool": tool,
                    "params": params,
                    "risk_level": risk_level
                }
            })
        }
    }
}
