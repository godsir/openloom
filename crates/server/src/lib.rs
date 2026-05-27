pub mod avatar;
pub mod blob;
pub mod dispatch;
pub mod jsonrpc;
pub mod proxy;
pub mod sse;
pub mod ws;

use anyhow::Result;
use axum::{Router, extract::State, routing::get};
use openloom_engine::Engine;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

pub struct Server {
    engine: Arc<Engine>,
    port: u16,
    #[allow(dead_code)]
    config_path: Option<PathBuf>,
}

impl Server {
    pub fn new(engine: Engine, config_path: Option<PathBuf>) -> Self {
        let engine = Arc::new(engine);
        engine.start_cron_scheduler();
        Self {
            engine,
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

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

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
            .route(
                "/api/upload-blob",
                axum::routing::post(blob::handle_upload_blob),
            )
            .route(
                "/api/avatar/{role}",
                axum::routing::get(avatar::serve_avatar).post(avatar::upload_avatar),
            )
            .layer(cors)
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

        // Write port/pid files for Electron sidecar lifecycle management
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("openLoom");
        let _ = fs::create_dir_all(&data_dir);

        // Clean stale port/pid from previous crashed instance
        let pid_path = data_dir.join("engine.pid");
        if pid_path.exists()
            && let Ok(pid_str) = fs::read_to_string(&pid_path)
            && let Ok(old_pid) = pid_str.trim().parse::<u32>()
            && old_pid != std::process::id()
        {
            let _ = fs::remove_file(&pid_path);
            let _ = fs::remove_file(data_dir.join("engine.port"));
            tracing::info!("cleaned up stale port/pid files from pid {}", old_pid);
        }

        // Write current port and pid
        let _ = fs::write(data_dir.join("engine.port"), bound_addr.port().to_string());
        let _ = fs::write(&pid_path, std::process::id().to_string());
        tracing::info!(port = bound_addr.port(), "port/pid files written");

        axum::serve(listener, app).await?;

        // Cleanup port/pid files on graceful shutdown
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("openLoom");
        let _ = fs::remove_file(data_dir.join("engine.port"));
        let _ = fs::remove_file(data_dir.join("engine.pid"));
        tracing::info!("port/pid files removed");
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
