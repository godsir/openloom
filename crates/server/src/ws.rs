use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use openloom_engine::Engine;
use openloom_models::*;
use std::sync::Arc;

use crate::dispatch;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(engine): State<Arc<Engine>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, engine))
}

async fn handle_ws(mut socket: WebSocket, engine: Arc<Engine>) {
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&text) {
                    let result = dispatch_ws(&engine, &req).await;
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

async fn dispatch_ws(
    engine: &Engine,
    req: &JsonRpcRequest,
) -> Result<serde_json::Value, JsonRpcError> {
    dispatch::dispatch_method(engine, &req.method, req.params.clone()).await
}
