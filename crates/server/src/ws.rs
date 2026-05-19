use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use openloom_engine::Engine;
use openloom_models::*;
use std::sync::Arc;

pub async fn ws_handler(ws: WebSocketUpgrade, State(engine): State<Arc<Engine>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, engine))
}

async fn handle_ws(mut socket: WebSocket, engine: Arc<Engine>) {
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&text) {
                    let result = dispatch_method(&engine, &req).await;
                    let resp = match result {
                        Ok(value) => JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            result: Some(value),
                            error: None,
                            id: req.id,
                        },
                        Err(err) => JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            result: None,
                            error: Some(err),
                            id: req.id,
                        },
                    };
                    if let Ok(json) = serde_json::to_string(&resp) {
                        let _ = socket.send(Message::Text(json)).await;
                    }
                } else {
                    let err = JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        result: None,
                        error: Some(JsonRpcError {
                            code: ErrorCode::ParseError,
                            message: "invalid JSON-RPC".into(),
                            data: None,
                        }),
                        id: 0,
                    };
                    if let Ok(json) = serde_json::to_string(&err) {
                        let _ = socket.send(Message::Text(json)).await;
                    }
                }
            }
            Message::Ping(_) => {
                let _ = socket.send(Message::Pong(vec![])).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

async fn dispatch_method(
    engine: &Engine,
    req: &JsonRpcRequest,
) -> Result<serde_json::Value, JsonRpcError> {
    match req.method.as_str() {
        "system.health" => {
            let health = engine.health_check().await;
            Ok(serde_json::to_value(health).unwrap_or_default())
        }
        "chat.send" => {
            let params = req.params.clone().unwrap_or_default();
            let content = params
                .get("messages")
                .and_then(|m| m.as_array())
                .and_then(|arr| arr.last())
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();

            let msg = ChatMessage {
                role: "user".into(),
                content: content.to_string(),
            };
            engine
                .handle_message(msg, &session_id)
                .await
                .map(|resp| serde_json::to_value(resp).unwrap_or_default())
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })
        }
        "skill.list" => {
            let skills: Vec<serde_json::Value> = engine
                .list_skills()
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "description": s.description,
                        "triggers": s.triggers,
                    })
                })
                .collect();
            Ok(serde_json::json!({"skills": skills}))
        }
        "skill.invoke" => {
            let params = req.params.clone().unwrap_or_default();
            let name = params.get("skill_name").and_then(|v| v.as_str()).unwrap_or("");
            let p = params.get("params").cloned().unwrap_or_default();
            engine
                .invoke_skill(name, p)
                .await
                .map(|r| serde_json::json!({"result": r}))
                .map_err(|e| JsonRpcError {
                    code: ErrorCode::SkillFailed,
                    message: e.to_string(),
                    data: None,
                })
        }
        "system.shutdown" => Ok(serde_json::json!({"ok": true})),
        "memory.query" => Ok(serde_json::json!({"events": [], "cognitions": []})),
        "memory.persona" => Ok(serde_json::json!({"summary": "Phase 2", "traits": []})),
        "agent.status" => Ok(serde_json::json!({"state": "idle"})),
        "cache.stats" => Ok(serde_json::json!({"hit_rate": 0.0, "block_count": 0, "total_size_mb": 0})),
        _ => Err(JsonRpcError {
            code: ErrorCode::MethodNotFound,
            message: format!("method '{}' not found", req.method),
            data: None,
        }),
    }
}
