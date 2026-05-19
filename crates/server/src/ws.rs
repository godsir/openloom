use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                if let Ok(req) =
                    serde_json::from_str::<openloom_models::JsonRpcRequest>(&text)
                {
                    let resp = openloom_models::JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        result: Some(serde_json::json!({"echo": req.method})),
                        error: None,
                        id: req.id,
                    };
                    if let Ok(json) = serde_json::to_string(&resp) {
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
