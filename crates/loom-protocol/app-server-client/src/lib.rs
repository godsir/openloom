//! App-server client facade for CLI surfaces.
//!
//! This crate provides the [`AppServerClient`] enum, which abstracts over:
//! - **Loom** (`LoomAppServerClient`): wraps openLoom Engine for in-process use.
//! - **Remote** (`RemoteAppServerClient`): WebSocket transport for
//!   connecting to a remote app-server (--remote flag).
//!
//! Both variants share the same typed request/notification/event model,
//! so CLI surfaces can switch between them without changing their
//! higher-level session logic.
//!
//! Full request_typed mapping will be implemented in Phase 8.1.
//! Currently the Loom variant returns `MethodNotFound` for all requests.

mod remote;

use std::error::Error;
use std::fmt;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::time::Duration;

use loom_app_server_protocol::ClientNotification;
use loom_app_server_protocol::ClientRequest;
use loom_app_server_protocol::JSONRPCErrorError;
use loom_app_server_protocol::RequestId;
use loom_app_server_protocol::Result as JsonRpcResult;
use loom_app_server_protocol::ServerNotification;
use loom_app_server_protocol::ServerRequest;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

pub use crate::remote::RemoteAppServerClient;
pub use crate::remote::RemoteAppServerConnectArgs;
pub use crate::remote::RemoteAppServerEndpoint;

// ─── Re-exports from stubs (kept for API compatibility) ───

pub use loom_tui_stubs::feedback::CodexFeedback;
pub type StateDbHandle = std::sync::Arc<loom_tui_stubs::state::StateRuntime>;

/// Default channel capacity used by in-process and remote transport layers.
pub const DEFAULT_IN_PROCESS_CHANNEL_CAPACITY: usize = 256;

/// Socket path for app-server control channel (stub -- no daemon in openLoom).
pub fn app_server_control_socket_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_default()
        .join("openLoom")
        .join("control.sock")
}

// ─── Shared types ───

pub(crate) const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Raw app-server request result for typed requests.
///
/// Successful responses travel through the JSON-RPC result envelope
/// produced by the protocol layer.
pub type RequestResult = std::result::Result<JsonRpcResult, JSONRPCErrorError>;

/// Events emitted by the app-server client for consumption by CLI surfaces.
#[derive(Debug, Clone)]
pub enum AppServerEvent {
    /// Consumer fell behind; `skipped` events were dropped.
    Lagged { skipped: usize },
    /// Server-to-client notification (e.g. transcript delta, completion).
    ServerNotification(ServerNotification),
    /// Server-to-client request (e.g. tool approval, user input).
    ServerRequest(ServerRequest),
    /// Connection was lost (remote transport only).
    Disconnected { message: String },
}

/// Layered error for typed request dispatch.
///
/// Keeps transport failures, server-side JSON-RPC failures, and response
/// decode failures distinct so callers can decide whether to retry, surface
/// a server error, or treat the response as an internal mismatch.
#[derive(Debug)]
pub enum TypedRequestError {
    Transport {
        method: String,
        source: IoError,
    },
    Server {
        method: String,
        source: JSONRPCErrorError,
    },
    Deserialize {
        method: String,
        source: serde_json::Error,
    },
    /// Returned when the method is not yet mapped in the Loom adapter (Phase 8.1).
    MethodNotFound,
}

impl fmt::Display for TypedRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport { method, source } => {
                write!(f, "{method} transport error: {source}")
            }
            Self::Server { method, source } => {
                write!(
                    f,
                    "{method} failed: {} (code {})",
                    source.message, source.code
                )?;
                if let Some(data) = source.data.as_ref() {
                    write!(f, ", data: {data}")?;
                }
                Ok(())
            }
            Self::Deserialize { method, source } => {
                write!(f, "{method} response decode error: {source}")
            }
            Self::MethodNotFound => {
                write!(f, "method not found (not yet mapped in Loom adapter)")
            }
        }
    }
}

impl Error for TypedRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport { source, .. } => Some(source),
            Self::Server { .. } => None,
            Self::Deserialize { source, .. } => Some(source),
            Self::MethodNotFound => None,
        }
    }
}

/// Extracts the JSON-RPC method name from a ClientRequest for diagnostics.
pub(crate) fn request_method_name(request: &ClientRequest) -> String {
    serde_json::to_value(request)
        .ok()
        .and_then(|value| {
            value
                .get("method")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "<unknown>".to_string())
}

/// Returns `true` for notifications that must survive backpressure.
///
/// Transcript events (AgentMessageDelta, PlanDelta, reasoning deltas) and
/// the authoritative ItemCompleted / TurnCompleted form the lossless tier
/// of the event stream. Everything else is best-effort and may be dropped
/// with only cosmetic impact.
#[allow(dead_code)]
pub(crate) fn server_notification_requires_delivery(notification: &ServerNotification) -> bool {
    matches!(
        notification,
        ServerNotification::TurnCompleted(_)
            | ServerNotification::ThreadSettingsUpdated(_)
            | ServerNotification::ItemCompleted(_)
            | ServerNotification::AgentMessageDelta(_)
            | ServerNotification::PlanDelta(_)
            | ServerNotification::ReasoningSummaryTextDelta(_)
            | ServerNotification::ReasoningTextDelta(_)
    )
}

// ─── LoomAppServerClient (wraps openLoom Engine) ───

/// In-process app-server client backed by the openLoom Engine.
///
/// This is the primary variant for local use. It wraps `openloom_engine::Engine`
/// and provides the same request/notification/event model as the original
/// `InProcessAppServerClient`, but routed through openLoom's internals instead
/// of the codex-app-server message processor.
pub struct LoomAppServerClient {
    _engine: openloom_engine::Engine,
    event_rx: mpsc::UnboundedReceiver<AppServerEvent>,
    _event_tx: mpsc::UnboundedSender<AppServerEvent>,
}

impl LoomAppServerClient {
    /// Creates a new in-process client wrapping the openLoom Engine.
    ///
    /// The engine is initialised with the provided config and can be used
    /// immediately for health checks, session creation, and (in Phase 8.1)
    /// typed request dispatch.
    pub async fn new(config: openloom_engine::EngineConfig) -> anyhow::Result<Self> {
        let engine = openloom_engine::Engine::new(config)?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Ok(Self {
            _engine: engine,
            event_rx,
            _event_tx: event_tx,
        })
    }

    /// Sends a typed client request and returns a deserialized response body.
    ///
    /// **Phase 4.2:** Returns `MethodNotFound` for all requests.
    /// Full mapping to Engine methods will be implemented in Phase 8.1.
    pub async fn request_typed<T: DeserializeOwned>(
        &self,
        _req: ClientRequest,
    ) -> Result<T, TypedRequestError> {
        Err(TypedRequestError::MethodNotFound)
    }

    /// Returns a handle that can be cloned and used concurrently for
    /// request dispatch.
    pub fn request_handle(&self) -> LoomAppServerRequestHandle {
        LoomAppServerRequestHandle
    }

    /// Sends a typed client notification (no response expected).
    pub async fn notify(&self, _notification: ClientNotification) -> IoResult<()> {
        // Notifications are no-ops until Phase 8.1
        Ok(())
    }

    /// Resolves a pending server request with a success result.
    pub async fn resolve_server_request(
        &self,
        _request_id: RequestId,
        _result: JsonRpcResult,
    ) -> IoResult<()> {
        Ok(())
    }

    /// Rejects a pending server request with a JSON-RPC error.
    pub async fn reject_server_request(
        &self,
        _request_id: RequestId,
        _error: JSONRPCErrorError,
    ) -> IoResult<()> {
        Ok(())
    }

    /// Returns the next event, or `None` if the event stream has ended.
    ///
    /// Callers should drain this stream promptly. If they fall behind,
    /// `AppServerEvent::Lagged` markers may be emitted to signal backpressure.
    pub fn next_event(&mut self) -> Option<AppServerEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Shuts down the engine and event stream.
    ///
    /// The underlying engine is dropped and the event channel is closed.
    pub async fn shutdown(self) -> IoResult<()> {
        drop(self._engine);
        Ok(())
    }
}

// ─── Loom Request Handle ───

/// Cloneable handle for request dispatch to the Loom engine.
///
/// In Phase 4.2 this is a no-op placeholder; full implementation
/// comes in Phase 8.1 when the typed request mapping is completed.
#[derive(Clone)]
pub struct LoomAppServerRequestHandle;

impl LoomAppServerRequestHandle {
    /// Sends a typed client request via this handle.
    pub async fn request_typed<T: DeserializeOwned>(
        &self,
        _req: ClientRequest,
    ) -> Result<T, TypedRequestError> {
        Err(TypedRequestError::MethodNotFound)
    }
}

/// Cloneable handle that abstracts over the Loom and Remote transport backends.
#[derive(Clone)]
pub enum AppServerRequestHandle {
    Loom(LoomAppServerRequestHandle),
    Remote(crate::remote::RemoteAppServerRequestHandle),
}

impl AppServerRequestHandle {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        match self {
            Self::Loom(handle) => handle.request(request).await,
            Self::Remote(handle) => handle.request(request).await,
        }
    }

    pub async fn request_typed<T: DeserializeOwned>(
        &self,
        request: ClientRequest,
    ) -> Result<T, TypedRequestError> {
        match self {
            Self::Loom(handle) => handle.request_typed(request).await,
            Self::Remote(handle) => handle.request_typed(request).await,
        }
    }
}

impl LoomAppServerRequestHandle {
    async fn request(&self, _request: ClientRequest) -> IoResult<RequestResult> {
        Err(IoError::new(
            ErrorKind::Unsupported,
            "loom request dispatch not yet implemented (Phase 8.1)",
        ))
    }
}

// ─── Top-level AppServerClient ───

/// The unified app-server client.
///
/// Callers instantiate either `AppServerClient::Loom(..)` for in-process use
/// or `AppServerClient::Remote(..)` for a WebSocket-based connection.
pub enum AppServerClient {
    Loom(Box<LoomAppServerClient>),
    Remote(RemoteAppServerClient),
}

impl AppServerClient {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        match self {
            Self::Loom(client) => client.request(request).await,
            Self::Remote(client) => client.request(request).await,
        }
    }

    pub async fn request_typed<T: DeserializeOwned>(
        &self,
        request: ClientRequest,
    ) -> Result<T, TypedRequestError> {
        match self {
            Self::Loom(client) => client.request_typed(request).await,
            Self::Remote(client) => client.request_typed(request).await,
        }
    }

    pub async fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        match self {
            Self::Loom(client) => client.notify(notification).await,
            Self::Remote(client) => client.notify(notification).await,
        }
    }

    pub async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: JsonRpcResult,
    ) -> IoResult<()> {
        match self {
            Self::Loom(client) => client.resolve_server_request(request_id, result).await,
            Self::Remote(client) => client.resolve_server_request(request_id, result).await,
        }
    }

    pub async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        match self {
            Self::Loom(client) => client.reject_server_request(request_id, error).await,
            Self::Remote(client) => client.reject_server_request(request_id, error).await,
        }
    }

    pub async fn next_event(&mut self) -> Option<AppServerEvent> {
        match self {
            Self::Loom(client) => client.next_event(),
            Self::Remote(client) => client.next_event().await,
        }
    }

    pub async fn shutdown(self) -> IoResult<()> {
        match self {
            Self::Loom(client) => client.shutdown().await,
            Self::Remote(client) => client.shutdown().await,
        }
    }

    pub fn request_handle(&self) -> AppServerRequestHandle {
        match self {
            Self::Loom(client) => AppServerRequestHandle::Loom(client.request_handle()),
            Self::Remote(client) => AppServerRequestHandle::Remote(client.request_handle()),
        }
    }
}

impl LoomAppServerClient {
    async fn request(&self, _request: ClientRequest) -> IoResult<RequestResult> {
        Err(IoError::new(
            ErrorKind::Unsupported,
            "loom request dispatch not yet implemented (Phase 8.1)",
        ))
    }
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_request_error_exposes_sources() {
        let transport = TypedRequestError::Transport {
            method: "config/read".to_string(),
            source: IoError::new(ErrorKind::BrokenPipe, "closed"),
        };
        assert!(std::error::Error::source(&transport).is_some());

        let server = TypedRequestError::Server {
            method: "thread/read".to_string(),
            source: JSONRPCErrorError {
                code: -32603,
                data: Some(serde_json::json!({"detail": "config lock mismatch"})),
                message: "internal".to_string(),
            },
        };
        assert!(std::error::Error::source(&server).is_none());
        assert_eq!(
            server.to_string(),
            "thread/read failed: internal (code -32603), data: {\"detail\":\"config lock mismatch\"}"
        );

        let deserialize = TypedRequestError::Deserialize {
            method: "thread/start".to_string(),
            source: serde_json::from_str::<u32>("\"nope\"")
                .expect_err("invalid integer should return deserialize error"),
        };
        assert!(std::error::Error::source(&deserialize).is_some());

        let not_found = TypedRequestError::MethodNotFound;
        assert!(std::error::Error::source(&not_found).is_none());
        assert!(
            not_found
                .to_string()
                .contains("not yet mapped in Loom adapter")
        );
    }

    #[test]
    fn server_notification_requires_delivery_marks_transcript_and_terminal_events() {
        assert!(server_notification_requires_delivery(
            &ServerNotification::TurnCompleted(
                loom_app_server_protocol::TurnCompletedNotification {
                    thread_id: "thread".to_string(),
                    turn: loom_app_server_protocol::Turn {
                        id: "turn".to_string(),
                        items_view: loom_app_server_protocol::TurnItemsView::Full,
                        items: Vec::new(),
                        status: loom_app_server_protocol::TurnStatus::Completed,
                        error: None,
                        started_at: None,
                        completed_at: Some(0),
                        duration_ms: None,
                    },
                }
            )
        ));
        assert!(server_notification_requires_delivery(
            &ServerNotification::AgentMessageDelta(
                loom_app_server_protocol::AgentMessageDeltaNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    item_id: "item".to_string(),
                    delta: "hello".to_string(),
                }
            )
        ));
        assert!(server_notification_requires_delivery(
            &ServerNotification::ItemCompleted(
                loom_app_server_protocol::ItemCompletedNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    completed_at_ms: 0,
                    item: loom_app_server_protocol::ThreadItem::AgentMessage {
                        id: "item".to_string(),
                        text: "hello".to_string(),
                        phase: None,
                        memory_citation: None,
                    },
                }
            )
        ));
        assert!(!server_notification_requires_delivery(
            &ServerNotification::CommandExecutionOutputDelta(
                loom_app_server_protocol::CommandExecutionOutputDeltaNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    item_id: "item".to_string(),
                    delta: "stdout".to_string(),
                }
            )
        ));
    }

    #[test]
    fn request_method_name_extracts_correctly() {
        let name = request_method_name(&ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: loom_app_server_protocol::GetAccountParams {
                refresh_token: false,
            },
        });
        assert_eq!(name, "account/read");
    }

    #[test]
    fn remote_auth_token_transport_policy_allows_wss_and_loopback_ws() {
        assert!(crate::remote::websocket_url_supports_auth_token(
            &url::Url::parse("wss://example.com:443").expect("wss URL should parse")
        ));
        assert!(crate::remote::websocket_url_supports_auth_token(
            &url::Url::parse("ws://127.0.0.1:4500").expect("loopback ws URL should parse")
        ));
        assert!(!crate::remote::websocket_url_supports_auth_token(
            &url::Url::parse("ws://example.com:4500").expect("non-loopback ws URL should parse")
        ));
    }

    #[test]
    fn default_channel_capacity_is_positive() {
        assert!(DEFAULT_IN_PROCESS_CHANNEL_CAPACITY > 0);
    }

    #[test]
    fn app_server_event_debug_is_available() {
        let event = AppServerEvent::Lagged { skipped: 1 };
        let s = format!("{event:?}");
        assert!(s.contains("Lagged"));
    }
}
