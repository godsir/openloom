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
//! Full request_typed mapping implemented in Phase 8.1.

mod remote;

use std::error::Error;
use std::fmt;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::sync::Arc;
use std::time::Duration;

use loom_app_server_protocol::ClientNotification;
use loom_app_server_protocol::ClientRequest;
use loom_app_server_protocol::JSONRPCErrorError;
use loom_app_server_protocol::RequestId;
use loom_app_server_protocol::Result as JsonRpcResult;
use loom_app_server_protocol::ServerNotification;
use loom_app_server_protocol::ServerRequest;
use openloom_models::ChatMessage;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

pub use crate::remote::RemoteAppServerClient;
pub use crate::remote::RemoteAppServerConnectArgs;
pub use crate::remote::RemoteAppServerEndpoint;

// ─── Re-exports from stubs (kept for API compatibility) ───

pub use loom_tui_stubs::feedback::CodexFeedback;
pub type StateDbHandle = std::sync::Arc<loom_tui_stubs::state::StateRuntime>;

/// Stub InProcessAppServerClient for TUI compatibility.
pub struct InProcessAppServerClient;

impl InProcessAppServerClient {
    pub async fn start(_args: InProcessClientStartArgs) -> std::io::Result<Self> {
        Ok(Self)
    }
    pub async fn shutdown(self) -> std::io::Result<()> {
        Ok(())
    }
    pub fn request_handle(&self) -> AppServerRequestHandle {
        AppServerRequestHandle::Loom(LoomAppServerRequestHandle::default())
    }
    pub async fn request(&self, _request: ClientRequest) -> std::io::Result<RequestResult> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "InProcess stub not wired",
        ))
    }
    pub async fn request_typed<T: DeserializeOwned>(
        &self,
        _request: ClientRequest,
    ) -> Result<T, TypedRequestError> {
        Err(TypedRequestError::MethodNotFound)
    }
}

/// Stub InProcessClientStartArgs for TUI compatibility.
#[derive(Debug, Clone)]
pub struct InProcessClientStartArgs {
    pub arg0_paths: loom_arg0::Arg0DispatchPaths,
    pub config: std::sync::Arc<loom_tui_stubs::config::Config>,
    pub cli_overrides: Vec<(String, toml::Value)>,
    pub loader_overrides: loom_config::LoaderOverrides,
    pub strict_config: bool,
    pub cloud_requirements: loom_config::CloudRequirementsLoader,
    pub feedback: CodexFeedback,
    pub log_db: Option<loom_tui_stubs::state::log_db::LogDbLayer>,
    pub state_db: Option<StateDbHandle>,
    pub environment_manager: std::sync::Arc<loom_exec_server::EnvironmentManager>,
    pub config_warnings: Vec<loom_app_server_protocol::ConfigWarningNotification>,
    pub session_source: serde_json::Value,
    pub enable_codex_api_key_env: bool,
    pub client_name: String,
    pub client_version: String,
    pub experimental_api: bool,
    pub opt_out_notification_methods: Vec<String>,
    pub channel_capacity: usize,
}
impl Default for InProcessClientStartArgs {
    fn default() -> Self {
        Self {
            arg0_paths: loom_arg0::Arg0DispatchPaths::default(),
            config: std::sync::Arc::new(loom_tui_stubs::config::Config::default()),
            cli_overrides: Vec::new(),
            loader_overrides: loom_config::LoaderOverrides::default(),
            strict_config: false,
            cloud_requirements: loom_config::CloudRequirementsLoader::default(),
            feedback: CodexFeedback::new(),
            log_db: None,
            state_db: None,
            environment_manager: std::sync::Arc::new(
                loom_exec_server::EnvironmentManager::default_for_tests(),
            ),
            config_warnings: Vec::new(),
            session_source: serde_json::json!("cli"),
            enable_codex_api_key_env: false,
            client_name: String::new(),
            client_version: String::new(),
            experimental_api: false,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        }
    }
}

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
    engine: Arc<openloom_engine::Engine>,
    event_rx: mpsc::UnboundedReceiver<AppServerEvent>,
    event_tx: mpsc::UnboundedSender<AppServerEvent>,
}

impl LoomAppServerClient {
    /// Creates a new in-process client wrapping the openLoom Engine.
    ///
    /// The engine is initialised with the provided config and can be used
    /// immediately for health checks, session creation, and (in Phase 8.1)
    /// typed request dispatch.
    pub async fn new(config: openloom_engine::EngineConfig) -> anyhow::Result<Self> {
        let engine = Arc::new(openloom_engine::Engine::new(config)?);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Ok(Self {
            engine,
            event_rx,
            event_tx,
        })
    }

    /// Sends a typed client request and returns a deserialized response body.
    pub async fn request_typed<T: DeserializeOwned>(
        &self,
        req: ClientRequest,
    ) -> Result<T, TypedRequestError> {
        dispatch_request::<T>(Arc::clone(&self.engine), self.event_tx.clone(), req).await
    }

    /// Returns a handle that can be cloned and used concurrently for
    /// request dispatch.
    pub fn request_handle(&self) -> LoomAppServerRequestHandle {
        LoomAppServerRequestHandle {
            engine: Arc::clone(&self.engine),
            event_tx: self.event_tx.clone(),
        }
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
        drop(self.engine);
        Ok(())
    }
}

// ─── Loom Request Handle ───

/// Cloneable handle for request dispatch to the Loom engine.
#[derive(Clone)]
pub struct LoomAppServerRequestHandle {
    engine: Arc<openloom_engine::Engine>,
    event_tx: mpsc::UnboundedSender<AppServerEvent>,
}

impl Default for LoomAppServerRequestHandle {
    fn default() -> Self {
        // Default handle is a no-op; real handles are created by LoomAppServerClient
        let (event_tx, _) = mpsc::unbounded_channel();
        // Note: default handle cannot create an engine, so request_typed will fail.
        // This is only used in tests and places where the handle is replaced immediately.
        Self {
            engine: Arc::new(
                openloom_engine::Engine::new_test(
                    std::env::temp_dir().join("loom_handle_default.db"),
                )
                .expect("default test engine"),
            ),
            event_tx,
        }
    }
}

impl LoomAppServerRequestHandle {
    /// Sends a typed client request via this handle.
    ///
    /// Delegates to the same dispatch as `LoomAppServerClient::request_typed`.
    pub async fn request_typed<T: DeserializeOwned>(
        &self,
        req: ClientRequest,
    ) -> Result<T, TypedRequestError> {
        dispatch_request::<T>(Arc::clone(&self.engine), self.event_tx.clone(), req).await
    }
}

/// Cloneable handle that abstracts over the Loom and Remote transport backends.
#[derive(Clone)]
pub enum AppServerRequestHandle {
    Loom(LoomAppServerRequestHandle),
    Remote(crate::remote::RemoteAppServerRequestHandle),
    #[doc(hidden)]
    InProcess(Box<AppServerRequestHandle>),
}

impl AppServerRequestHandle {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        match self {
            Self::Loom(handle) => handle.request(request).await,
            Self::Remote(handle) => handle.request(request).await,
            Self::InProcess(_handle) => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "InProcess stub: use Loom handle",
            )),
        }
    }

    pub async fn request_typed<T: DeserializeOwned>(
        &self,
        request: ClientRequest,
    ) -> Result<T, TypedRequestError> {
        match self {
            Self::Loom(handle) => handle.request_typed(request).await,
            Self::Remote(handle) => handle.request_typed(request).await,
            Self::InProcess(_handle) => Err(TypedRequestError::MethodNotFound),
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
    #[doc(hidden)]
    InProcess(InProcessAppServerClient),
}

impl AppServerClient {
    pub async fn request(&self, request: ClientRequest) -> IoResult<RequestResult> {
        match self {
            Self::Loom(client) => client.request(request).await,
            Self::Remote(client) => client.request(request).await,
            Self::InProcess(client) => client.request(request).await,
        }
    }

    pub async fn request_typed<T: DeserializeOwned>(
        &self,
        request: ClientRequest,
    ) -> Result<T, TypedRequestError> {
        match self {
            Self::Loom(client) => client.request_typed(request).await,
            Self::Remote(client) => client.request_typed(request).await,
            Self::InProcess(client) => client.request_typed(request).await,
        }
    }

    pub async fn notify(&self, notification: ClientNotification) -> IoResult<()> {
        match self {
            Self::Loom(client) => client.notify(notification).await,
            Self::Remote(client) => client.notify(notification).await,
            Self::InProcess(_) => Ok(()),
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
            Self::InProcess(_) => Ok(()),
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
            Self::InProcess(_) => Ok(()),
        }
    }

    pub async fn next_event(&mut self) -> Option<AppServerEvent> {
        match self {
            Self::Loom(client) => client.next_event(),
            Self::Remote(client) => client.next_event().await,
            Self::InProcess(_) => None,
        }
    }

    pub async fn shutdown(self) -> IoResult<()> {
        match self {
            Self::Loom(client) => client.shutdown().await,
            Self::Remote(client) => client.shutdown().await,
            Self::InProcess(client) => client.shutdown().await,
        }
    }

    pub fn request_handle(&self) -> AppServerRequestHandle {
        match self {
            Self::Loom(client) => AppServerRequestHandle::Loom(client.request_handle()),
            Self::Remote(client) => AppServerRequestHandle::Remote(client.request_handle()),
            Self::InProcess(client) => {
                AppServerRequestHandle::InProcess(Box::new(client.request_handle()))
            }
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

// ─── Request dispatch helpers ───

/// Core dispatch: maps ClientRequest variants to engine calls.
async fn dispatch_request<T: DeserializeOwned>(
    engine: Arc<openloom_engine::Engine>,
    event_tx: mpsc::UnboundedSender<AppServerEvent>,
    req: ClientRequest,
) -> Result<T, TypedRequestError> {
    let method = req.method();
    match req {
        // ─── Turn lifecycle (CRITICAL) ───
        ClientRequest::TurnStart { params, .. } => {
            let text = extract_user_input_text(&params.input);
            let turn_id = uuid::Uuid::new_v4().to_string();
            let item_id = uuid::Uuid::new_v4().to_string();
            let session_id = params.thread_id.clone();
            let thread_id = params.thread_id.clone();

            let msg = ChatMessage {
                role: "user".into(),
                content: text,
                timestamp: chrono::Utc::now(),
            };

            // Clone IDs for the spawned task
            let spawned_turn_id = turn_id.clone();
            let spawned_item_id = item_id.clone();
            let spawned_thread_id = thread_id.clone();

            // Spawn streaming bridge: tokens from engine become AgentMessageDelta events
            let e = Arc::clone(&engine);
            let tx = event_tx.clone();

            tokio::spawn(async move {
                let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(64);

                // Forward tokens to event stream
                let forward_tx = tx.clone();
                let fwd_tid = spawned_thread_id.clone();
                let fwd_turn_inner = spawned_turn_id.clone();
                let fwd_turn_outer = spawned_turn_id;
                let fwd_item = spawned_item_id;
                let forward_handle = tokio::spawn(async move {
                    while let Some(token) = token_rx.recv().await {
                        let _ = forward_tx.send(AppServerEvent::ServerNotification(
                            ServerNotification::AgentMessageDelta(
                                loom_app_server_protocol::AgentMessageDeltaNotification {
                                    thread_id: fwd_tid.clone(),
                                    turn_id: fwd_turn_inner.clone(),
                                    item_id: fwd_item.clone(),
                                    delta: token,
                                },
                            ),
                        ));
                    }
                });

                // Run the model
                let _ = e
                    .handle_message_streaming(
                        msg,
                        &session_id,
                        token_tx,
                        openloom_models::Mode::Code,
                        openloom_models::ModelPreference::Auto,
                        openloom_models::ThinkingLevel::default(),
                    )
                    .await;

                // Wait for forwarder to drain
                let _ = forward_handle.await;

                // Send TurnCompleted
                let _ = tx.send(AppServerEvent::ServerNotification(
                    ServerNotification::TurnCompleted(
                        loom_app_server_protocol::TurnCompletedNotification {
                            thread_id: spawned_thread_id,
                            turn: loom_app_server_protocol::Turn {
                                id: fwd_turn_outer,
                                items: vec![],
                                items_view: loom_app_server_protocol::TurnItemsView::Full,
                                status: loom_app_server_protocol::TurnStatus::Completed,
                                error: None,
                                started_at: None,
                                completed_at: Some(
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs() as i64,
                                ),
                                duration_ms: None,
                            },
                        },
                    ),
                ));
            });

            // Return TurnStartResponse immediately with the generated turn
            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let turn = loom_app_server_protocol::Turn {
                id: turn_id,
                items: vec![],
                items_view: loom_app_server_protocol::TurnItemsView::Full,
                status: loom_app_server_protocol::TurnStatus::InProgress,
                error: None,
                started_at: Some(now_ts),
                completed_at: None,
                duration_ms: None,
            };
            respond(
                &method,
                loom_app_server_protocol::TurnStartResponse { turn },
            )
        }

        ClientRequest::TurnInterrupt { .. } => {
            engine.interrupt();
            respond(&method, loom_app_server_protocol::TurnInterruptResponse {})
        }

        // ─── Thread lifecycle ───
        ClientRequest::ThreadStart { params, .. } => {
            let session =
                engine
                    .create_session()
                    .await
                    .map_err(|e| TypedRequestError::Transport {
                        method: method.clone(),
                        source: IoError::new(ErrorKind::Other, e.to_string()),
                    })?;
            let thread = make_thread_stub(&session.id, params.model_provider.as_deref());
            let cwd = loom_absolute_path::AbsolutePathBuf::from_absolute_path(
                std::env::current_dir().unwrap_or_default(),
            )
            .unwrap_or_else(|_| {
                loom_absolute_path::AbsolutePathBuf::from_absolute_path("/").unwrap()
            });
            let response = loom_app_server_protocol::ThreadStartResponse {
                thread,
                model: params.model.unwrap_or_else(|| "qwen3-1.7b".into()),
                model_provider: params.model_provider.unwrap_or_else(|| "local".into()),
                service_tier: None,
                cwd,
                runtime_workspace_roots: vec![],
                instruction_sources: vec![],
                approval_policy: loom_app_server_protocol::AskForApproval::Never,
                approvals_reviewer: loom_app_server_protocol::ApprovalsReviewer::User,
                sandbox: loom_app_server_protocol::SandboxPolicy::DangerFullAccess,
                active_permission_profile: None,
                reasoning_effort: None,
            };
            respond(&method, response)
        }

        ClientRequest::ThreadList { .. } => {
            let sessions =
                engine
                    .list_sessions()
                    .await
                    .map_err(|e| TypedRequestError::Transport {
                        method: method.clone(),
                        source: IoError::new(ErrorKind::Other, e.to_string()),
                    })?;
            let threads: Vec<loom_app_server_protocol::Thread> = sessions
                .into_iter()
                .map(|s| make_thread_stub(&s.id, Some("local")))
                .collect();
            respond(
                &method,
                loom_app_server_protocol::ThreadListResponse {
                    data: threads,
                    next_cursor: None,
                    backwards_cursor: None,
                },
            )
        }

        ClientRequest::ThreadRead { params, .. } => {
            let thread = make_thread_stub(&params.thread_id, Some("local"));
            respond(
                &method,
                loom_app_server_protocol::ThreadReadResponse { thread },
            )
        }

        ClientRequest::ThreadResume { params, .. } => {
            let thread = make_thread_stub(&params.thread_id, params.model_provider.as_deref());
            let cwd = loom_absolute_path::AbsolutePathBuf::from_absolute_path(
                params.cwd.as_deref().unwrap_or("."),
            )
            .unwrap_or_else(|_| {
                loom_absolute_path::AbsolutePathBuf::from_absolute_path("/").unwrap()
            });
            let response = loom_app_server_protocol::ThreadResumeResponse {
                thread,
                model: params.model.unwrap_or_else(|| "qwen3-1.7b".into()),
                model_provider: params.model_provider.unwrap_or_else(|| "local".into()),
                service_tier: None,
                cwd,
                runtime_workspace_roots: vec![],
                instruction_sources: vec![],
                approval_policy: loom_app_server_protocol::AskForApproval::Never,
                approvals_reviewer: loom_app_server_protocol::ApprovalsReviewer::User,
                sandbox: loom_app_server_protocol::SandboxPolicy::DangerFullAccess,
                active_permission_profile: None,
                reasoning_effort: None,
            };
            respond(&method, response)
        }

        ClientRequest::ThreadFork { params, .. } => {
            let new_session =
                engine
                    .create_session()
                    .await
                    .map_err(|e| TypedRequestError::Transport {
                        method: method.clone(),
                        source: IoError::new(ErrorKind::Other, e.to_string()),
                    })?;
            let thread = make_thread_stub(&new_session.id, params.model_provider.as_deref());
            let cwd = loom_absolute_path::AbsolutePathBuf::from_absolute_path(
                std::env::current_dir().unwrap_or_default(),
            )
            .unwrap_or_else(|_| {
                loom_absolute_path::AbsolutePathBuf::from_absolute_path("/").unwrap()
            });
            let response = loom_app_server_protocol::ThreadForkResponse {
                thread,
                model: params.model.unwrap_or_else(|| "qwen3-1.7b".into()),
                model_provider: params.model_provider.unwrap_or_else(|| "local".into()),
                service_tier: None,
                cwd,
                runtime_workspace_roots: vec![],
                instruction_sources: vec![],
                approval_policy: loom_app_server_protocol::AskForApproval::Never,
                approvals_reviewer: loom_app_server_protocol::ApprovalsReviewer::User,
                sandbox: loom_app_server_protocol::SandboxPolicy::DangerFullAccess,
                active_permission_profile: None,
                reasoning_effort: None,
            };
            respond(&method, response)
        }

        ClientRequest::ThreadSetName { .. } => {
            respond(&method, loom_app_server_protocol::ThreadSetNameResponse {})
        }

        ClientRequest::ThreadSettingsUpdate { .. } => respond(
            &method,
            loom_app_server_protocol::ThreadSettingsUpdateResponse {},
        ),

        // ─── Skills ───
        ClientRequest::SkillsList { .. } => {
            let skills = engine.list_skills();
            let skill_metas: Vec<loom_app_server_protocol::SkillMetadata> = skills
                .into_iter()
                .map(|s| loom_app_server_protocol::SkillMetadata {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    short_description: None,
                    interface: None,
                    dependencies: None,
                    path: loom_absolute_path::AbsolutePathBuf::from_absolute_path(
                        std::env::current_dir().unwrap_or_default(),
                    )
                    .unwrap_or_else(|_| {
                        loom_absolute_path::AbsolutePathBuf::from_absolute_path("/").unwrap()
                    }),
                    scope: loom_app_server_protocol::SkillScope::User,
                    enabled: true,
                })
                .collect();
            let entry = loom_app_server_protocol::SkillsListEntry {
                cwd: std::env::current_dir().unwrap_or_default(),
                skills: skill_metas,
                errors: vec![],
            };
            respond(
                &method,
                loom_app_server_protocol::SkillsListResponse { data: vec![entry] },
            )
        }

        // ─── Models ───
        ClientRequest::ModelList { .. } => {
            let display_name = engine.model_display_name();
            let model_entries: Vec<loom_app_server_protocol::Model> =
                vec![loom_app_server_protocol::Model {
                    id: "openloom-local".into(),
                    model: display_name.clone(),
                    upgrade: None,
                    upgrade_info: None,
                    availability_nux: None,
                    display_name,
                    description: "openLoom local model".into(),
                    hidden: false,
                    supported_reasoning_efforts: vec![],
                    default_reasoning_effort: loom_protocol::openai_models::ReasoningEffort::Medium,
                    input_modalities: vec![],
                    supports_personality: false,
                    additional_speed_tiers: vec![],
                    service_tiers: vec![],
                    default_service_tier: None,
                    is_default: true,
                }];
            respond(
                &method,
                loom_app_server_protocol::ModelListResponse {
                    data: model_entries,
                    next_cursor: None,
                },
            )
        }

        // ─── Account ───
        ClientRequest::GetAccount { .. } => {
            let account = loom_app_server_protocol::Account::ApiKey {};
            respond(
                &method,
                loom_app_server_protocol::GetAccountResponse {
                    account: Some(account),
                    requires_openai_auth: false,
                },
            )
        }

        ClientRequest::LogoutAccount { .. } => {
            respond(&method, loom_app_server_protocol::LogoutAccountResponse {})
        }

        // ─── Config ───
        ClientRequest::ConfigBatchWrite { .. } => respond(
            &method,
            loom_app_server_protocol::ConfigWriteResponse {
                status: loom_app_server_protocol::WriteStatus::Ok,
                version: String::new(),
                file_path: loom_absolute_path::AbsolutePathBuf::from_absolute_path(
                    dirs::data_dir()
                        .unwrap_or_default()
                        .join("openLoom")
                        .join("config.toml"),
                )
                .unwrap_or_else(|_| {
                    loom_absolute_path::AbsolutePathBuf::from_absolute_path("/").unwrap()
                }),
                overridden_metadata: None,
            },
        ),

        ClientRequest::ConfigRead { .. } => {
            respond(
                &method,
                loom_app_server_protocol::ConfigReadResponse {
                    config: serde_json::from_value(serde_json::json!({})).unwrap_or_else(|_| {
                        // Fallback: construct a minimal config
                        loom_app_server_protocol::Config {
                            model: None,
                            review_model: None,
                            model_context_window: None,
                            model_auto_compact_token_limit: None,
                            model_auto_compact_token_limit_scope: None,
                            model_provider: None,
                            approval_policy: None,
                            approvals_reviewer: None,
                            sandbox_mode: None,
                            sandbox_workspace_write: None,
                            forced_chatgpt_workspace_id: None,
                            forced_login_method: None,
                            web_search: None,
                            tools: None,
                            profile: None,
                            profiles: std::collections::HashMap::new(),
                            instructions: None,
                            developer_instructions: None,
                            compact_prompt: None,
                            model_reasoning_effort: None,
                            model_reasoning_summary: None,
                            model_verbosity: None,
                            service_tier: None,
                            analytics: None,
                            apps: None,
                            desktop: None,
                            additional: std::collections::HashMap::new(),
                        }
                    }),
                    origins: std::collections::HashMap::new(),
                    layers: None,
                },
            )
        }

        // ─── Plugin stubs ───
        ClientRequest::PluginList { .. } => respond(
            &method,
            loom_app_server_protocol::PluginListResponse {
                marketplaces: vec![],
                marketplace_load_errors: vec![],
                featured_plugin_ids: vec![],
            },
        ),

        ClientRequest::PluginInstalled { .. } => respond(
            &method,
            loom_app_server_protocol::PluginInstalledResponse {
                marketplaces: vec![],
                marketplace_load_errors: vec![],
            },
        ),

        ClientRequest::HooksList { .. } => respond(
            &method,
            loom_app_server_protocol::HooksListResponse { data: vec![] },
        ),

        // ─── Catch-all: not yet mapped ───
        _ => Err(TypedRequestError::MethodNotFound),
    }
}

/// Serialize a concrete response type to JSON and deserialize into the caller's generic T.
fn respond<T: DeserializeOwned, R: Serialize>(
    method: &str,
    response: R,
) -> Result<T, TypedRequestError> {
    let json = serde_json::to_value(response).map_err(|e| TypedRequestError::Deserialize {
        method: method.to_string(),
        source: e,
    })?;
    serde_json::from_value(json).map_err(|e| TypedRequestError::Deserialize {
        method: method.to_string(),
        source: e,
    })
}

/// Extract the text content from the first Text variant in the user input list.
fn extract_user_input_text(inputs: &[loom_app_server_protocol::UserInput]) -> String {
    for input in inputs {
        if let loom_app_server_protocol::UserInput::Text { text, .. } = input {
            return text.clone();
        }
    }
    String::new()
}

/// Build a minimal stub Thread struct from a session id.
fn make_thread_stub(id: &str, model_provider: Option<&str>) -> loom_app_server_protocol::Thread {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let cwd = loom_absolute_path::AbsolutePathBuf::from_absolute_path(
        std::env::current_dir().unwrap_or_default(),
    )
    .unwrap_or_else(|_| loom_absolute_path::AbsolutePathBuf::from_absolute_path("/").unwrap());
    loom_app_server_protocol::Thread {
        id: id.to_string(),
        session_id: id.to_string(),
        forked_from_id: None,
        preview: String::new(),
        ephemeral: false,
        model_provider: model_provider.unwrap_or("local").to_string(),
        created_at: now,
        updated_at: now,
        status: loom_app_server_protocol::ThreadStatus::Idle,
        path: None,
        cwd,
        cli_version: env!("CARGO_PKG_VERSION").to_string(),
        source: loom_app_server_protocol::SessionSource::Cli,
        thread_source: None,
        agent_nickname: None,
        agent_role: None,
        git_info: None,
        name: None,
        turns: vec![],
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
