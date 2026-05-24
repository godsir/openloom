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
                        // Try JSON-RPC first
                        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&text) {
                            let method = req.method.clone();
                            // For chat.send, spawn a background task so the event loop
                            // can keep forwarding stream_delta events while the LLM streams.
                            if method == "chat.send" {
                                let engine_clone = engine.clone();
                                let req_id = req.id;
                                let params = req.params.clone();
                                let event_bus = engine.event_bus().clone();
                                tokio::spawn(async move {
                                    let result = dispatch::dispatch_method(&engine_clone, "chat.send", params).await;
                                    // The dispatch already fires StreamDelta/StreamEnd events.
                                    // Signal completion via a sentinel StreamEnd in case dispatch
                                    // returned without one (e.g. error path).
                                    let _ = result; // result already sent via events
                                    // Notify the event bus that rpc id is done (ignored by frontend)
                                    let _ = event_bus.send(EngineEvent::AgentStateChanged {
                                        old_state: AgentState::Acting,
                                        new_state: AgentState::Idle,
                                    });
                                });
                                // Immediately ACK the RPC so the frontend knows the request was received
                                let ack = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "result": {"ok": true, "streaming": true},
                                    "id": req_id,
                                });
                                if let Ok(json) = serde_json::to_string(&ack) {
                                    let _ = socket.send(Message::Text(json)).await;
                                }
                            } else {
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
                            }
                        } else if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&text) {
                            // Handle openhanako-style raw messages (type: 'prompt', type: 'steer', etc.)
                            let msg_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            match msg_type {
                                "prompt" => {
                                    let content = raw.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let session_id = raw.get("sessionPath").and_then(|v| v.as_str()).unwrap_or("default").to_string();
                                    let engine_clone = engine.clone();
                                    // Spawn so stream_delta events can flow while LLM is running
                                    tokio::spawn(async move {
                                        let msg = ChatMessage {
                                            role: "user".into(),
                                            content: content.clone(),
                                            timestamp: chrono::Utc::now(),
            id: None,
            seq: None,
                                            metadata: None,
                                        };
                                        let _ = engine_clone.handle_message(msg, &session_id, Mode::Chat, ModelPreference::default()).await;
                                    });
                                }
                                "steer" => {
                                    // Steer is for mid-stream input, not yet supported
                                    let _ = socket.send(Message::Text(serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "method": "chat.steer",
                                        "params": {"ok": false, "reason": "not supported"}
                                    }).to_string())).await;
                                }
                                "context_usage" => {
                                    let session_id = raw.get("sessionPath").and_then(|v| v.as_str()).unwrap_or("");
                                    let (used, total, percent) = engine.context_usage(session_id).await;
                                    let _ = socket.send(Message::Text(serde_json::json!({
                                        "type": "context_usage",
                                        "sessionPath": session_id,
                                        "tokens": used,
                                        "contextWindow": total,
                                        "percent": percent,
                                    }).to_string())).await;
                                }
                                _ => {
                                    let err = JsonRpcResponse {
                                        jsonrpc: "2.0".into(), result: None,
                                        error: Some(JsonRpcError {
                                            code: ErrorCode::MethodNotFound, message: format!("unknown message type: {}", msg_type), data: None,
                                        }),
                                        id: 0,
                                    };
                                    if let Ok(json) = serde_json::to_string(&err) {
                                        let _ = socket.send(Message::Text(json)).await;
                                    }
                                }
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
        EngineEvent::StreamDelta { session_id, delta } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "chat.stream_delta",
                "params": { "session_id": session_id, "delta": delta }
            })
        }
        EngineEvent::StreamEnd { session_id, full_response } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "chat.stream_end",
                "params": { "session_id": session_id, "response": full_response }
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
        EngineEvent::ToolCallStarted {
            session_id,
            call_id,
            name,
            arguments,
        } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tool.start",
                "params": {
                    "session_id": session_id,
                    "call_id": call_id,
                    "name": name,
                    "args": arguments
                }
            })
        }
        EngineEvent::ToolCallEnded {
            session_id,
            call_id,
            name,
            success,
            result_summary,
        } => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tool.end",
                "params": {
                    "session_id": session_id,
                    "call_id": call_id,
                    "name": name,
                    "success": success,
                    "details": { "summary": result_summary }
                }
            })
        }
    }
}
