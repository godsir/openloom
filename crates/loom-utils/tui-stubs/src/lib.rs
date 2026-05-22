use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

// ─── codex-feedback ───
pub mod feedback {
    use super::*;
    use loom_protocol::ThreadId;
    use tracing::Subscriber;
    use tracing_subscriber::layer::Layer;
    use tracing_subscriber::registry::LookupSpan;

    #[derive(Clone, Debug)]
    pub struct CodexFeedback;

    impl Default for CodexFeedback {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CodexFeedback {
        pub fn new() -> Self {
            Self
        }
        pub fn make_writer(&self) -> FeedbackMakeWriter {
            FeedbackMakeWriter
        }
        pub fn logger_layer<S>(&self) -> impl Layer<S> + Send + Sync + 'static
        where
            S: Subscriber + for<'a> LookupSpan<'a>,
        {
            tracing_subscriber::layer::Identity::new()
        }
        pub fn metadata_layer<S>(&self) -> impl Layer<S> + Send + Sync + 'static
        where
            S: Subscriber + for<'a> LookupSpan<'a>,
        {
            tracing_subscriber::layer::Identity::new()
        }
        pub fn snapshot(&self, _session_id: Option<ThreadId>) -> FeedbackSnapshot {
            FeedbackSnapshot
        }
    }

    pub struct FeedbackMakeWriter;
    impl io::Write for FeedbackMakeWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FeedbackMakeWriter {
        type Writer = Self;
        fn make_writer(&self) -> Self::Writer {
            FeedbackMakeWriter
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct FeedbackSnapshot;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct FeedbackDiagnostics;
    pub const DOCTOR_REPORT_ATTACHMENT_FILENAME: &str = "doctor-report.txt";
    pub const FEEDBACK_DIAGNOSTICS_ATTACHMENT_FILENAME: &str = "feedback-diagnostics.json";

    #[derive(Debug, Clone, Default)]
    pub struct FeedbackDiagnostic;
}

// ─── codex-state ───
pub mod state {
    use super::*;

    #[derive(Clone, Debug)]
    pub struct StateRuntime;
    impl StateRuntime {
        pub async fn new(_path: PathBuf) -> Result<Self, anyhow::Error> {
            Ok(Self)
        }
        pub fn get_thread(&self, _id: loom_protocol::ThreadId) -> Option<()> { None }
    }

    pub fn state_db_path(codex_home: &Path) -> PathBuf {
        codex_home.join("state.db")
    }

    pub async fn try_init(_config: &super::config::Config) -> Result<Arc<StateRuntime>, anyhow::Error> {
        Ok(Arc::new(StateRuntime))
    }

    pub async fn get_state_db(_config: &super::config::Config) -> Option<Arc<StateRuntime>> {
        Some(Arc::new(StateRuntime))
    }

    pub mod log_db {
        use super::*;
        #[derive(Clone)]
        pub struct LogDbLayer;
        pub async fn start(_state_db: Arc<super::StateRuntime>) -> LogDbLayer {
            LogDbLayer
        }
    }
}

// ─── codex-rollout ───
pub mod rollout {
    use super::*;
    pub type StateDbHandle = Arc<state::StateRuntime>;
    pub mod state_db {
        pub use super::StateDbHandle;

        pub async fn try_init(config: &super::super::config::Config) -> Result<std::sync::Arc<super::super::state::StateRuntime>, anyhow::Error> {
            Ok(std::sync::Arc::new(super::super::state::StateRuntime))
        }

        pub async fn get_state_db(config: &super::super::config::Config) -> Option<std::sync::Arc<super::super::state::StateRuntime>> {
            Some(std::sync::Arc::new(super::super::state::StateRuntime))
        }
    }
}

// ─── codex-message-history ───
pub mod message_history {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct HistoryConfig {
        pub codex_home: PathBuf,
        pub persistence: HistoryPersistence,
        pub max_bytes: Option<usize>,
    }

    #[derive(Debug, Clone)]
    pub enum HistoryPersistence {
        SaveOnEveryMessage,
        SaveNever,
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
    pub struct HistoryEntry {
        pub session_id: String,
        pub ts: u64,
        pub text: String,
    }

    impl HistoryConfig {
        pub fn new(codex_home: impl Into<PathBuf>, _: &()) -> Self {
            Self {
                codex_home: codex_home.into(),
                persistence: HistoryPersistence::SaveNever,
                max_bytes: None,
            }
        }
    }

    pub async fn history_metadata(_config: &HistoryConfig) -> (u64, usize) {
        (0, 0)
    }
    pub async fn append_entry(
        _text: &str,
        _id: impl std::fmt::Display,
        _config: &HistoryConfig,
    ) -> io::Result<()> {
        Ok(())
    }
    pub fn lookup(_log_id: u64, _offset: usize, _config: &HistoryConfig) -> Option<HistoryEntry> {
        None
    }
}

// ─── codex-plugin ───
pub mod plugin {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct AppConnectorId(pub String);

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct PluginId {
        pub name: String,
        pub marketplace: String,
    }
    impl PluginId {
        pub fn new(name: impl Into<String>, marketplace: impl Into<String>) -> Self {
            Self { name: name.into(), marketplace: marketplace.into() }
        }
    }

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct PluginCapabilitySummary {
        pub config_name: String,
        pub display_name: String,
        pub description: Option<String>,
        pub has_skills: bool,
        pub mcp_server_names: Vec<String>,
        pub app_connector_ids: Vec<AppConnectorId>,
    }
}

// ─── codex-connectors ───
pub mod connectors {
    use loom_app_server_protocol::AppInfo;

    /// Uses AppInfo from app-server-protocol
    pub fn connector_display_label(connector: &AppInfo) -> String {
        connector.name.clone()
    }
    pub fn connector_mention_slug(connector: &AppInfo) -> String {
        connector.name.to_lowercase().replace(' ', "-")
    }

    // Additional codex-core::connectors stubs
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    pub struct ConnectorDirectoryCacheContext;
    impl ConnectorDirectoryCacheContext {
        pub fn new() -> Self { Self }
    }
    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    pub struct ConnectorDirectoryCacheKey(pub String);
    impl ConnectorDirectoryCacheKey {
        pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }
    }
    #[derive(Debug, Clone)]
    pub struct DirectoryListResponse;

    pub mod filter {
        use super::AppInfo;
        pub fn filter_disallowed_connectors(_connectors: Vec<AppInfo>, _enabled: bool) -> Vec<AppInfo> {
            vec![]
        }
    }

    pub mod merge {
        use super::AppInfo;
        pub fn merge_connectors(_a: Vec<AppInfo>, _b: Vec<AppInfo>) -> Vec<AppInfo> {
            vec![]
        }
        pub fn merge_plugin_connectors(_a: Vec<AppInfo>, _b: &crate::plugin::PluginCapabilitySummary) -> Vec<AppInfo> {
            vec![]
        }
    }

    pub mod metadata {
        use super::AppInfo;
        pub fn connector_install_url(_connector: &AppInfo) -> Option<String> {
            None
        }
        pub fn connector_display_label(connector: &AppInfo) -> String {
            connector.name.clone()
        }
        pub fn connector_mention_slug(connector: &AppInfo) -> String {
            connector.name.to_lowercase().replace(' ', "-")
        }
    }

    pub async fn list_accessible_connectors_from_mcp_tools(
        _config: &super::Config,
    ) -> anyhow::Result<Vec<AppInfo>> {
        Ok(vec![])
    }

    pub async fn list_accessible_connectors_from_mcp_tools_with_environment_manager(
        _config: &super::Config,
        _env_manager: &Arc<loom_exec_server::EnvironmentManager>,
    ) -> anyhow::Result<Vec<AppInfo>> {
        Ok(vec![])
    }

    pub async fn list_accessible_connectors_from_mcp_tools_with_options(
        _config: &super::Config,
        _env_manager: &Arc<loom_exec_server::EnvironmentManager>,
        _cache_context: Option<ConnectorDirectoryCacheContext>,
    ) -> anyhow::Result<Vec<AppInfo>> {
        Ok(vec![])
    }

    pub async fn list_accessible_connectors_from_mcp_tools_with_options_and_status(
        _config: &super::Config,
        _env_manager: &Arc<loom_exec_server::EnvironmentManager>,
        _cache_context: Option<ConnectorDirectoryCacheContext>,
    ) -> anyhow::Result<(Vec<AppInfo>, Vec<(AppInfo, bool)>)> {
        Ok((vec![], vec![]))
    }

    pub async fn list_cached_accessible_connectors_from_mcp_tools(
        _cache_key: &ConnectorDirectoryCacheKey,
    ) -> anyhow::Result<DirectoryListResponse> {
        Ok(DirectoryListResponse)
    }

    pub async fn with_app_enabled_state(
        _config: &super::Config,
        _connectors: Vec<AppInfo>,
    ) -> Vec<(AppInfo, bool)> {
        vec![]
    }
}

// ─── codex-model-provider ───
pub mod model_provider {
    pub struct ModelProvider;

    impl ModelProvider {
        pub async fn new() -> Self {
            Self
        }
        pub fn runtime_base_url(&self) -> Option<String> { None }
    }

    pub async fn create_model_provider() -> ModelProvider {
        ModelProvider
    }
}

// ─── codex-model-provider-info (re-export from config bridge) ───
pub mod model_provider_info {
    pub use loom_config::model_info::*;
}

// ─── codex-models-manager ───
pub mod models_manager {
    pub mod model_presets {
        pub const HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG: &str =
            "hide_gpt-5.1-codex-max_migration_prompt";
        pub const HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG: &str = "hide_gpt5_1_migration_prompt";
    }
    pub mod collaboration_mode_presets {
        pub fn builtin_collaboration_mode_presets() -> Vec<()> {
            vec![]
        }
    }
}

// ─── codex-cloud-requirements ───
pub mod cloud_requirements {
    pub fn cloud_requirements_loader_for_storage() {}
}

// ─── codex-core ───
// Config re-exported from the config sub-module below.
pub use config::Config;

use loom_protocol::ThreadId;

pub use loom_absolute_path::AbsolutePathBuf;

/// Stub thread type.
#[derive(Clone)]
pub struct CodexThread;

/// Stub new thread params.
pub struct NewThread;

/// Stub thread manager.
#[derive(Clone)]
pub struct ThreadManager;

impl ThreadManager {
    pub async fn new() -> Self {
        Self
    }
    pub async fn start_thread(&self, _thread: NewThread) -> Result<CodexThread, anyhow::Error> {
        Ok(CodexThread)
    }
    pub async fn resolve_thread(&self, _id: ThreadId) -> Option<CodexThread> {
        Some(CodexThread)
    }
}

pub type StateDbHandle = Arc<state::StateRuntime>;

pub fn resolve_installation_id(_codex_home: &std::path::Path) -> String {
    String::new()
}

pub fn check_execpolicy_for_warnings(_config: &Config) -> Vec<String> {
    vec![]
}

pub fn format_exec_policy_error_with_source(_err: &anyhow::Error) -> String {
    String::new()
}

pub fn find_thread_meta_by_name_str(_name: &str, _config: &Config) -> Option<(ThreadId, String)> {
    None
}

pub mod path_utils {
    use std::path::Path;
    pub fn resolve_codex_home_path(_path: &Path) -> std::path::PathBuf {
        Path::new(".").to_path_buf()
    }
}

pub mod config {
    use loom_absolute_path::AbsolutePathBuf;
    use std::path::PathBuf;

    // ═══ Pre-declare stub types needed by Config fields ═══

    #[derive(Debug, Clone, Default)]
    pub struct Constrained<T>(pub T);

    impl<T: Clone + PartialEq> Constrained<T> {
        pub fn value(&self) -> T { self.0.clone() }
        pub fn set(&mut self, value: T) -> ConstraintResult<()> { self.0 = value; Ok(()) }
        pub fn can_set(&self, _value: &T) -> bool { true }
    }

    #[derive(Debug, Clone)]
    pub struct ConstraintError(pub String);

    impl std::fmt::Display for ConstraintError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
    impl std::error::Error for ConstraintError {}

    pub type ConstraintResult<T = ()> = Result<T, ConstraintError>;

    #[derive(Debug, Clone)]
    pub struct PermissionProfileSnapshot {
        pub id: String,
    }
    impl Default for PermissionProfileSnapshot {
        fn default() -> Self { Self { id: String::new() } }
    }

    #[derive(Debug, Clone)]
    pub struct TerminalResizeReflowConfig {
        pub enabled: bool,
        pub max_rows: Option<TerminalResizeReflowMaxRows>,
    }
    impl Default for TerminalResizeReflowConfig {
        fn default() -> Self { Self { enabled: true, max_rows: None } }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct TerminalResizeReflowMaxRows(pub usize);

    #[derive(Debug, Clone, Copy, Default)]
    pub struct NetworkProxySpec;

    #[derive(Debug, Clone, Default)]
    pub struct ConfigLayerStackStub;

    impl ConfigLayerStackStub {
        pub fn effective_config(&self) -> toml_edit::ImDocument<String> {
            toml_edit::ImDocument::parse(String::new()).unwrap()
        }
        pub fn get_layers(&self) -> Vec<()> { vec![] }
        pub fn get_active_user_layer(&self) -> Option<()> { None }
        pub fn effective_user_config(&self) -> toml_edit::ImDocument<String> {
            toml_edit::ImDocument::parse(String::new()).unwrap()
        }
    }

    // Simple stub types used by Config fields (avoiding external deps)

    #[derive(Debug, Clone)]
    pub struct PermissionsStub {
        pub approval_policy: Constrained<loom_protocol::protocol::AskForApproval>,
        pub network: Option<NetworkProxySpec>,
        pub windows_sandbox_mode: Option<String>,
    }

    impl Default for PermissionsStub {
        fn default() -> Self {
            Self {
                approval_policy: Constrained(loom_protocol::protocol::AskForApproval::OnRequest),
                network: None,
                windows_sandbox_mode: None,
            }
        }
    }

    impl PermissionsStub {
        pub fn effective_permission_profile(&self) -> loom_protocol::models::PermissionProfile {
            loom_protocol::models::PermissionProfile::default()
        }
        pub fn legacy_sandbox_policy(&self, _cwd: &std::path::Path) -> loom_protocol::protocol::SandboxPolicy {
            loom_protocol::protocol::SandboxPolicy::DangerFullAccess
        }
        pub fn permission_profile(&self) -> loom_protocol::models::PermissionProfile {
            loom_protocol::models::PermissionProfile::default()
        }
        pub fn active_permission_profile(&self) -> Option<loom_protocol::models::ActivePermissionProfile> {
            None
        }
        pub fn set_permission_profile_from_session_snapshot<P1, P2>(&mut self, _profile: P1, _active_profile: P2) -> ConstraintResult<()> { Ok(()) }
        pub fn set_workspace_roots(&mut self, _roots: &[loom_absolute_path::AbsolutePathBuf]) {}
        pub fn replace_permission_profile_from_session_snapshot(&mut self, _snapshot: &PermissionProfileSnapshot) {}
        pub fn user_visible_workspace_roots(&self) -> Vec<loom_absolute_path::AbsolutePathBuf> { vec![] }
    }

    #[derive(Debug, Clone, Default)]
    pub struct NetworkStub;
    impl NetworkStub {
        pub fn as_ref(&self) -> &Self { self }
    }

    #[derive(Debug, Clone, Default)]
    pub struct ApprovalsReviewerStub;

    pub use loom_features::Features;
    pub use loom_features::Feature;

    /// ManagedFeatures stub that wraps real Features with constraint checking.
    #[derive(Debug, Clone)]
    pub struct ManagedFeaturesStub {
        value: loom_features::Features,
    }

    impl Default for ManagedFeaturesStub {
        fn default() -> Self {
            Self { value: loom_features::Features::with_defaults() }
        }
    }

    impl std::ops::Deref for ManagedFeaturesStub {
        type Target = loom_features::Features;
        fn deref(&self) -> &Self::Target { &self.value }
    }

    impl ManagedFeaturesStub {
        pub fn get(&self) -> &loom_features::Features { &self.value }
        pub fn set_enabled(&mut self, feature: loom_features::Feature, enabled: bool) -> ConstraintResult<()> {
            self.value.set_enabled(feature, enabled);
            Ok(())
        }
        pub fn enable(&mut self, feature: loom_features::Feature) -> ConstraintResult<()> {
            self.value.enable(feature);
            Ok(())
        }
        pub fn disable(&mut self, feature: loom_features::Feature) -> ConstraintResult<()> {
            self.value.disable(feature);
            Ok(())
        }
        pub fn can_set(&self, _candidate: &loom_features::Features) -> ConstraintResult<()> { Ok(()) }
        pub fn set(&mut self, candidate: loom_features::Features) -> ConstraintResult<()> {
            self.value = candidate;
            Ok(())
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct NotificationsStub {
        pub method: Option<String>,
        pub condition: Option<String>,
        pub notifications: Vec<String>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct ExternalConfigMigrationPromptsStub {
        pub home: Option<bool>,
        pub home_last_prompted_at: Option<i64>,
        pub projects: std::collections::BTreeMap<String, bool>,
        pub project_last_prompted_at: std::collections::BTreeMap<String, i64>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct NoticesTomlStub {
        pub hide_full_access_warning: Option<bool>,
        pub hide_world_writable_warning: Option<bool>,
        pub hide_gpt5_1_migration_prompt: Option<bool>,
        pub hide_gpt_5_1_codex_max_migration_prompt: Option<bool>,
        pub hide_rate_limit_model_nudge: Option<bool>,
        pub model_migrations: Vec<String>,
        pub external_config_migration_prompts: ExternalConfigMigrationPromptsStub,
        pub fast_default_opt_out: Option<bool>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct ModelAvailabilityNuxStub {
        pub shown_count: u32,
    }

    #[derive(Debug, Clone, Default)]
    pub struct ShellEnvironmentPolicyStub;

    #[derive(Debug, Clone, Default)]
    pub struct PersonalityStub;

    #[derive(Debug, Clone, Default)]
    pub struct ProjectConfigStub {
        pub trust_level: Option<loom_protocol::config_types::TrustLevel>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct MemoriesTomlStub {
        pub generate_memories: bool,
        pub use_memories: bool,
    }

    #[derive(Debug, Clone)]
    pub struct ModelProviderInfoStub {
        pub name: String,
        pub base_url: Option<String>,
        pub disabled_reason: Option<String>,
        pub wire_api: String,
        pub requires_openai_auth: bool,
        pub responses_api_overhead_ms: u64,
        pub responses_api_engine_iapi_ttft_ms: u64,
        pub responses_api_engine_iapi_tbt_ms: u64,
        pub responses_api_engine_service_ttft_ms: u64,
        pub responses_api_engine_service_tbt_ms: u64,
        pub responses_api_inference_time_ms: u64,
    }
    impl Default for ModelProviderInfoStub {
        fn default() -> Self {
            Self {
                name: String::new(),
                base_url: None,
                disabled_reason: None,
                wire_api: String::new(),
                requires_openai_auth: false,
                responses_api_overhead_ms: 0,
                responses_api_engine_iapi_ttft_ms: 0,
                responses_api_engine_iapi_tbt_ms: 0,
                responses_api_engine_service_ttft_ms: 0,
                responses_api_engine_service_tbt_ms: 0,
                responses_api_inference_time_ms: 0,
            }
        }
    }
    impl ModelProviderInfoStub {
        pub fn is_openai(&self) -> bool { self.name == "OpenAI" }
    }

    /// Comprehensive stub Config for TUI compilation.
    #[derive(Debug, Clone)]
    pub struct Config {
        // Paths
        pub cwd: AbsolutePathBuf,
        pub codex_home: AbsolutePathBuf,
        pub sqlite_home: PathBuf,
        pub log_dir: PathBuf,

        // Model
        pub model: Option<String>,
        pub model_provider: ModelProviderInfoStub,
        pub model_provider_id: String,
        pub model_reasoning_effort: Option<loom_protocol::openai_models::ReasoningEffort>,
        pub model_reasoning_summary: Option<loom_protocol::config_types::ReasoningSummary>,
        pub model_context_window: Option<i64>,
        pub model_verbosity: Option<loom_protocol::openai_models::ReasoningEffort>,
        pub service_tier: Option<String>,

        // Permissions / sandbox
        pub permissions: PermissionsStub,
        pub approvals_reviewer: loom_protocol::config_types::ApprovalsReviewer,
        pub enforce_residency: Constrained<Option<String>>,

        // Features
        pub features: ManagedFeaturesStub,
        pub animations: bool,
        pub show_tooltips: bool,
        pub tui_alternate_screen: loom_protocol::config_types::AltScreenMode,
        pub tui_keymap: loom_config::types::TuiKeymap,
        pub tui_notifications: NotificationsStub,

        // Stack
        pub config_layer_stack: ConfigLayerStackStub,

        // Warnings / personality
        pub startup_warnings: Vec<String>,
        pub personality: Option<loom_protocol::config_types::Personality>,
        pub base_instructions: Option<String>,
        pub developer_instructions: Option<String>,

        // Project
        pub active_project: loom_config::config_toml::ProjectConfig,

        // Misc
        pub chatgpt_base_url: String,
        pub cli_auth_credentials_store_mode: String,
        pub forced_login_method: Option<String>,
        pub forced_chatgpt_workspace_id: Option<String>,
        pub disable_paste_burst: bool,
        pub ephemeral: bool,
        pub feedback_enabled: bool,
        pub history: bool,
        pub memories: MemoriesTomlStub,
        pub terminal_resize_reflow: TerminalResizeReflowConfig,
        pub notices: NoticesTomlStub,
        pub otel: loom_otel_stub::SessionTelemetry,
        pub mcp_servers: Vec<String>,
        pub show_raw_agent_reasoning: bool,

        // TUI-specific
        pub tui_theme: Option<String>,
        pub tui_pet: Option<String>,
        pub tui_pet_anchor: Option<String>,
        pub tui_status_line: Option<String>,
        pub tui_status_line_use_colors: bool,
        pub tui_terminal_title: Option<String>,
        pub tui_vim_mode_default: Option<String>,
        pub tui_session_picker_view: Option<String>,
        pub tui_raw_output_mode: bool,

        // Workspace
        pub workspace_roots: Vec<AbsolutePathBuf>,
        pub web_search_mode: Constrained<loom_protocol::config_types::WebSearchMode>,

        // Realtime
        pub realtime: Option<loom_config::config_toml::RealtimeToml>,
        pub realtime_audio: loom_config::config_toml::RealtimeAudioToml,

        // Other
        pub plan_mode_reasoning_effort: Option<String>,
        pub model_availability_nux: ModelAvailabilityNuxStub,
        pub network: Option<NetworkProxySpec>,
        pub shell_environment_policy: ShellEnvironmentPolicyStub,
    }

    impl Config {
        pub fn effective_permission_profile(&self) -> loom_protocol::models::PermissionProfile {
            loom_protocol::models::PermissionProfile::default()
        }

        #[cfg(target_os = "windows")]
        pub fn set_windows_sandbox_enabled(&mut self, _value: bool) {}
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                cwd: AbsolutePathBuf::current_dir().unwrap_or_else(|_| {
                    AbsolutePathBuf::try_from(std::path::PathBuf::from(".")).unwrap()
                }),
                codex_home: AbsolutePathBuf::current_dir().unwrap_or_else(|_| {
                    AbsolutePathBuf::try_from(std::path::PathBuf::from(".")).unwrap()
                }),
                sqlite_home: PathBuf::from("."),
                log_dir: PathBuf::from("."),
                model: None,
                model_provider: ModelProviderInfoStub::default(),
                model_provider_id: String::new(),
                model_reasoning_effort: None,
                model_reasoning_summary: None,
                model_context_window: None,
                model_verbosity: None,
                service_tier: None,
                permissions: PermissionsStub {
                    approval_policy: Constrained(loom_protocol::protocol::AskForApproval::OnRequest),
                    network: None,
                    windows_sandbox_mode: None,
                },
                approvals_reviewer: loom_protocol::config_types::ApprovalsReviewer::default(),
                enforce_residency: Constrained(None),
                features: ManagedFeaturesStub::default(),
                animations: true,
                show_tooltips: true,
                tui_alternate_screen: loom_protocol::config_types::AltScreenMode::Auto,
                tui_keymap: loom_config::types::TuiKeymap::default(),
                tui_notifications: NotificationsStub::default(),
                config_layer_stack: ConfigLayerStackStub::default(),
                startup_warnings: Vec::new(),
                personality: None,
                base_instructions: None,
                developer_instructions: None,
                active_project: loom_config::config_toml::ProjectConfig { trust_level: None },
                chatgpt_base_url: String::new(),
                cli_auth_credentials_store_mode: String::new(),
                forced_login_method: None,
                forced_chatgpt_workspace_id: None,
                disable_paste_burst: false,
                ephemeral: false,
                feedback_enabled: false,
                history: true,
                memories: MemoriesTomlStub::default(),
                terminal_resize_reflow: TerminalResizeReflowConfig::default(),
                notices: NoticesTomlStub::default(),
                otel: loom_otel_stub::SessionTelemetry::default(),
                mcp_servers: Vec::new(),
                show_raw_agent_reasoning: false,
                tui_theme: None,
                tui_pet: None,
                tui_pet_anchor: None,
                tui_status_line: None,
                tui_status_line_use_colors: false,
                tui_terminal_title: None,
                tui_vim_mode_default: None,
                tui_session_picker_view: None,
                tui_raw_output_mode: false,
                workspace_roots: Vec::new(),
                web_search_mode: Constrained(loom_protocol::config_types::WebSearchMode::Cached),
                realtime: None,
                realtime_audio: loom_config::config_toml::RealtimeAudioToml::default(),
                plan_mode_reasoning_effort: None,
                model_availability_nux: ModelAvailabilityNuxStub::default(),
                network: None,
                shell_environment_policy: ShellEnvironmentPolicyStub::default(),
            }
        }
    }

    /// Stub ConfigBuilder with builder pattern methods.
    #[derive(Debug, Clone, Default)]
    pub struct ConfigBuilder {
        codex_home: Option<PathBuf>,
        overrides: ConfigOverrides,
        loader_overrides: LoaderOverrides,
        strict: bool,
        cloud_requirements: Option<CloudRequirementsLoader>,
        fallback_cwd: Option<PathBuf>,
    }

    impl ConfigBuilder {
        pub fn new() -> Self { Self::default() }
        pub fn codex_home(mut self, path: PathBuf) -> Self { self.codex_home = Some(path); self }
        pub fn cli_overrides(mut self, _overrides: Vec<(String, toml::Value)>) -> Self { self }
        pub fn harness_overrides(mut self, overrides: ConfigOverrides) -> Self { self.overrides = overrides; self }
        pub fn loader_overrides(mut self, overrides: LoaderOverrides) -> Self { self.loader_overrides = overrides; self }
        pub fn strict_config(mut self, strict: bool) -> Self { self.strict = strict; self }
        pub fn cloud_requirements(mut self, cr: CloudRequirementsLoader) -> Self { self.cloud_requirements = Some(cr); self }
        pub fn fallback_cwd(mut self, cwd: Option<PathBuf>) -> Self { self.fallback_cwd = cwd; self }
        pub async fn build(self) -> Result<Config, ConfigLoadError> { Ok(Config::default()) }
    }

    /// Stub ConfigOverrides with all fields.
    #[derive(Debug, Clone, Default)]
    pub struct ConfigOverrides {
        pub model: Option<String>,
        pub approval_policy: Option<loom_protocol::protocol::AskForApproval>,
        pub sandbox_mode: Option<loom_protocol::config_types::SandboxMode>,
        pub cwd: Option<PathBuf>,
        pub model_provider: Option<String>,
        pub codex_self_exe: Option<PathBuf>,
        pub codex_linux_sandbox_exe: Option<PathBuf>,
        pub main_execve_wrapper_exe: Option<PathBuf>,
        pub show_raw_agent_reasoning: Option<bool>,
        pub bypass_hook_trust: Option<bool>,
        pub additional_writable_roots: Vec<String>,
        pub permission_profile: Option<String>,
        pub default_permissions: Option<String>,
    }

    /// Stub ConfigLoadError — re-exported from loom_config.
    pub use loom_config::ConfigLoadError;
    pub use loom_config::ConfigLoadOptions;

    /// Stub LoaderOverrides — re-exported from loom_config.
    pub use loom_config::LoaderOverrides;

    /// Stub CloudRequirementsLoader.
    #[derive(Debug, Clone, Default)]
    pub struct CloudRequirementsLoader;

    // Config edit stubs
    pub mod edit {
        #[derive(Debug, Clone)]
        pub struct ConfigEditsBuilder;
        impl ConfigEditsBuilder {
            pub fn new() -> Self { Self }
            pub fn for_config(_config: &super::Config) -> Self { Self }
            pub fn set_session_picker_view(self, _view: &str) -> Self { self }
            pub fn set_realtime_speaker(self, _name: String) -> Self { self }
            pub fn set_realtime_microphone(self, _name: String) -> Self { self }
            pub fn set_model_availability_nux_count(self, _count: u32) -> Self { self }
            pub fn set_hide_world_writable_warning(self) -> Self { self }
            pub fn set_hide_rate_limit_model_nudge(self) -> Self { self }
            pub fn set_hide_full_access_warning(self) -> Self { self }
            pub fn record_model_migration_seen(self, _model: &str) -> Self { self }
            pub fn set_realtime_audio_device(self, _kind: &str, _name: String) -> Self { self }
            pub async fn apply(self) -> Result<(), anyhow::Error> { Ok(()) }
            pub fn with_edits(self, _edits: impl IntoIterator<Item = ConfigEdit>) -> Self { self }
        }

        #[derive(Debug, Clone)]
        pub enum ConfigEdit {
            SetNoticeExternalConfigMigrationPromptHomeLastPromptedAt(i64),
            SetNoticeExternalConfigMigrationPromptProjectLastPromptedAt(String, i64),
            SetNoticeHideExternalConfigMigrationPromptHome(bool),
            SetNoticeHideExternalConfigMigrationPromptProject(bool),
            SetModelAvailabilityNuxCount(u32),
            SetRealtimeSpeaker(String),
            SetRealtimeMicrophone(String),
            SetRealtimeAudioDevice(String, String),
            SetHideWorldWritableWarning,
            SetHideRateLimitModelNudge,
            SetHideFullAccessWarning,
            RecordModelMigrationSeen(String),
            SetPath {
                segments: Vec<String>,
                value: toml::Value,
            },
            Other,
        }

        pub fn keymap_bindings_edit() -> ConfigEdit { ConfigEdit::Other }
        pub fn keymap_binding_clear_edit() -> ConfigEdit { ConfigEdit::Other }
        pub fn status_line_items_edit() -> ConfigEdit { ConfigEdit::Other }
        pub fn status_line_use_colors_edit() -> ConfigEdit { ConfigEdit::Other }
        pub fn syntax_theme_edit() -> ConfigEdit { ConfigEdit::Other }
        pub fn terminal_title_items_edit() -> ConfigEdit { ConfigEdit::Other }
        pub fn tui_pet_edit() -> ConfigEdit { ConfigEdit::Other }
    }

    // Additional config stubs
    pub fn format_config_error_with_source(_err: &anyhow::Error) -> String { String::new() }
    pub fn find_codex_home() -> Option<AbsolutePathBuf> { None }
    pub fn set_project_trust_level() {}
    pub fn set_default_oss_provider() {}
    pub fn resolve_oss_provider(_provider: Option<&str>, _config: &()) -> Option<String> { None }

    pub fn load_config_as_toml_with_cli_and_load_options(
        _codex_home: &std::path::Path,
        _config_cwd: Option<&AbsolutePathBuf>,
        _cli_kv_overrides: Vec<(String, toml::Value)>,
        _options: loom_config::ConfigLoadOptions,
    ) -> Result<loom_config::config_toml::ConfigToml, ConfigLoadError> {
        Ok(loom_config::config_toml::ConfigToml::default())
    }

    pub fn resolve_profile_v2_config_path(
        _codex_home: &std::path::Path,
        _profile_v2: &str,
    ) -> AbsolutePathBuf {
        AbsolutePathBuf::current_dir().unwrap()
    }
}

// ─── codex-login ───
pub mod login {
    use std::path::PathBuf;

    #[derive(Debug, Clone)]
    pub struct AuthConfig {
        pub auth_credentials_store_mode: String,
        pub chatgpt_base_url: Option<String>,
        pub codex_home: PathBuf,
        pub forced_login_method: Option<String>,
        pub forced_chatgpt_workspace_id: Option<String>,
    }
    impl Default for AuthConfig {
        fn default() -> Self {
            Self {
                auth_credentials_store_mode: String::new(),
                chatgpt_base_url: None,
                codex_home: PathBuf::from("."),
                forced_login_method: None,
                forced_chatgpt_workspace_id: None,
            }
        }
    }
    pub fn enforce_login_restrictions() {}
    pub fn set_default_client_residency_requirement() {}
    pub fn read_openai_api_key_from_env() -> Option<String> {
        None
    }
    pub mod default_client {
        pub fn originator() -> String {
            String::new()
        }
        pub fn set_default_client_residency_requirement() {}
    }
}

// ─── codex-realtime-webrtc (non-Linux) ───
pub mod realtime_webrtc {
    pub struct RealtimeWebrtcSession;
    #[derive(Debug)]
    pub struct RealtimeWebrtcSessionHandle;
    #[derive(Debug, Clone)]
    pub enum RealtimeWebrtcEvent {
        Connected,
        Closed,
        Failed(String),
        LocalAudioLevel(u16),
    }
}

// ─── codex-windows-sandbox (cfg-gated) ───
#[cfg(target_os = "windows")]
pub mod windows_sandbox {
    pub struct WindowsSandbox;
    impl WindowsSandbox {
        pub fn new() -> Self {
            Self
        }
    }
    pub fn apply_world_writable_scan_and_denies() {}
}

// ═══ Comprehensive remaining stubs (appended) ═══

// RealtimeWebrtcSession
impl realtime_webrtc::RealtimeWebrtcSession {
    pub fn start() -> realtime_webrtc::RealtimeWebrtcSession { realtime_webrtc::RealtimeWebrtcSession }
}

impl realtime_webrtc::RealtimeWebrtcSessionHandle {
    pub fn apply_answer_sdp(&self) {}
    pub fn local_audio_peak(&self) -> u16 { 0 }
    pub fn close(&self) {}
}

// AuthConfig fields
impl login::AuthConfig {
    pub fn new() -> Self { Self::default() }
}

// NetworkProxySpec methods
impl config::NetworkProxySpec {
    pub fn socks_enabled() -> bool { false }
}
impl PartialEq for config::NetworkProxySpec {
    fn eq(&self, _other: &Self) -> bool { true }
}
impl Eq for config::NetworkProxySpec {}

// PermissionProfileSnapshot
impl config::PermissionProfileSnapshot {
    pub fn active<P: Clone>(_profile: P) -> Self { Self::default() }
    pub fn from_session_snapshot<P1, P2>(_profile: P1, _active: P2) -> Self { Self::default() }
}

// Constrained — additional factory
impl<T: Clone> config::Constrained<T> {
    pub fn allow_only(_value: T) -> Self { config::Constrained::<T>(_value) }
    pub fn allow_any(value: T) -> Self { config::Constrained::<T>(value) }
}

// TerminalResizeReflowMaxRows associated constants
impl config::TerminalResizeReflowMaxRows {
    pub const Auto: config::TerminalResizeReflowMaxRows = config::TerminalResizeReflowMaxRows(0);
    pub const Disabled: config::TerminalResizeReflowMaxRows = config::TerminalResizeReflowMaxRows(0);
    pub const Limit: config::TerminalResizeReflowMaxRows = config::TerminalResizeReflowMaxRows(0);
}

// Config methods
impl config::Config {
    pub fn effective_workspace_roots(&self) -> Vec<loom_absolute_path::AbsolutePathBuf> { vec![] }
    pub fn plugins_config_input(&self) -> () { () }
    pub fn set_windows_elevated_sandbox_enabled(&mut self, _val: bool) {}
    pub fn legacy_sandbox_policy(&self, _cwd: &std::path::Path) -> loom_protocol::protocol::SandboxPolicy {
        loom_protocol::protocol::SandboxPolicy::DangerFullAccess
    }
}

// Feedback diagnostics
impl feedback::FeedbackDiagnostics {
    pub fn diagnostics(&self) -> &Self { self }
    pub fn is_empty(&self) -> bool { true }
}
impl feedback::FeedbackSnapshot {
    pub fn feedback_diagnostics(&self) -> feedback::FeedbackDiagnostics { feedback::FeedbackDiagnostics }
}

// PluginId IntoIterator
impl<'a> IntoIterator for &'a plugin::PluginId {
    type Item = &'a plugin::PluginId;
    type IntoIter = std::iter::Once<&'a plugin::PluginId>;
    fn into_iter(self) -> Self::IntoIter { std::iter::once(self) }
}

    impl<'a> IntoIterator for &'a feedback::FeedbackDiagnostics {
        type Item = &'a feedback::FeedbackDiagnostic;
        type IntoIter = std::iter::Empty<&'a feedback::FeedbackDiagnostic>;
        fn into_iter(self) -> Self::IntoIter { std::iter::empty() }
    }
