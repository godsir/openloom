pub mod dispatch;
pub mod jsonrpc;
pub mod sse;
pub mod ws;

use anyhow::Result;
use axum::{Router, extract::State, routing::get};
use openloom_engine::Engine;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Server {
    engine: Arc<Engine>,
    port: u16,
    #[allow(dead_code)]
    config_path: Option<PathBuf>,
}

impl Server {
    pub fn new(engine: Engine, config_path: Option<PathBuf>) -> Self {
        Self {
            engine: Arc::new(engine),
            port: 0,
            config_path,
        }
    }

    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    pub async fn serve(mut self, port: u16) -> Result<()> {
        self.port = port;

        let engine = self.engine.clone();

        let app = Router::new()
            .route(
                "/health",
                get(|State(state): State<Arc<Engine>>| async move {
                    let health = state.health_check().await;
                    axum::Json(serde_json::to_value(health).unwrap_or_default())
                }),
            )
            .route("/ws", get(ws::ws_handler))
            .route("/sse/{session_id}", get(sse::sse_handler))
            .route("/api", axum::routing::post(jsonrpc::handle_jsonrpc))
            .with_state(engine);

        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        tracing::info!("server starting on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        let bound_addr = listener.local_addr()?;

        // Ready signal for Electron sidecar mode
        let ready = serde_json::json!({
            "type": "ready",
            "port": bound_addr.port(),
        });
        println!("{}", ready);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_parse_valid() {
        let json = r#"{"jsonrpc":"2.0","method":"system.health","params":null,"id":1}"#;
        let req: openloom_models::JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "system.health");
    }

    #[test]
    fn test_jsonrpc_parse_invalid() {
        let json = r#"not json"#;
        let result: Result<openloom_models::JsonRpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_jsonrpc_error_response() {
        let err = openloom_models::JsonRpcError {
            code: openloom_models::ErrorCode::MethodNotFound,
            message: "method not found".into(),
            data: None,
        };
        let resp = openloom_models::JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(err),
            id: 1,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("-32601"));
    }

    #[test]
    fn test_notification_name_mapping() {
        let event = openloom_models::EngineEvent::CognitionUpdated {
            trait_name: "risk".into(),
            old_value: "low".into(),
            new_value: "high".into(),
            confidence: 0.9,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("cognition_updated"));
    }
}
