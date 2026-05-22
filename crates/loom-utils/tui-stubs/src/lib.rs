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

    #[derive(Clone)]
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

    #[derive(Clone)]
    pub struct StateRuntime;
    impl StateRuntime {
        pub async fn new(_path: PathBuf) -> Result<Self, anyhow::Error> {
            Ok(Self)
        }
    }

    pub fn state_db_path(codex_home: &Path) -> PathBuf {
        codex_home.join("state.db")
    }

    pub mod log_db {
        use super::*;
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
    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    pub struct ConnectorDirectoryCacheKey(pub String);
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

    /// Stub Config type for the runtime configuration.
    #[derive(Debug, Clone)]
    pub struct Config {
        pub cwd: loom_absolute_path::AbsolutePathBuf,
        pub model_provider: String,
        pub model_provider_id: String,
        pub model_reasoning_effort: Option<String>,
        pub model_reasoning_summary: Option<String>,
        pub permissions: loom_protocol::models::PermissionProfile,
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                cwd: AbsolutePathBuf::current_dir().unwrap_or_else(|_| {
                    AbsolutePathBuf::try_from(std::path::PathBuf::from(".")).unwrap()
                }),
                model_provider: String::new(),
                model_provider_id: String::new(),
                model_reasoning_effort: None,
                model_reasoning_summary: None,
                permissions: loom_protocol::models::PermissionProfile::default(),
            }
        }
    }

    /// Stub ConfigBuilder.
    #[derive(Debug, Clone, Default)]
    pub struct ConfigBuilder;

    impl ConfigBuilder {
        pub fn new() -> Self {
            Self
        }
        pub fn build(self) -> Config {
            Config::default()
        }
    }

    /// Stub ConfigOverrides.
    #[derive(Debug, Clone, Default)]
    pub struct ConfigOverrides;

    /// Stub ConfigLoadError.
    #[derive(Debug)]
    pub struct ConfigLoadError;

    impl std::fmt::Display for ConfigLoadError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "config load error")
        }
    }

    impl std::error::Error for ConfigLoadError {}

    /// Stub ConfigLoadOptions.
    #[derive(Debug, Clone, Default)]
    pub struct ConfigLoadOptions;

    /// Stub LoaderOverrides.
    #[derive(Debug, Clone, Default)]
    pub struct LoaderOverrides;

    /// Stub: formats a config error.
    pub fn format_config_error_with_source(_err: &anyhow::Error) -> String {
        String::new()
    }

    /// Stub: finds codex home directory.
    pub fn find_codex_home() -> Option<AbsolutePathBuf> {
        None
    }

    /// Stub: loads config as TOML.
    pub fn load_config_as_toml_with_cli_and_load_options(
        _overrides: ConfigOverrides,
        _cli_config_overrides: &loom_cli_utils::CliConfigOverrides,
        _options: ConfigLoadOptions,
    ) -> Result<toml_edit::ImDocument<String>, ConfigLoadError> {
        Ok(toml_edit::ImDocument::parse(String::new()).unwrap())
    }

    /// Stub: resolves OSS provider.
    pub fn resolve_oss_provider(_config: &Config) -> Option<String> {
        None
    }

    /// Stub: resolves profile v2 config path.
    pub fn resolve_profile_v2_config_path() -> Option<AbsolutePathBuf> {
        None
    }
}

// ─── codex-login ───
pub mod login {
    pub struct AuthConfig;
    pub fn enforce_login_restrictions() {}
    pub fn set_default_client_residency_requirement() {}
    pub fn read_openai_api_key_from_env() -> Option<String> {
        None
    }
    pub mod default_client {
        pub fn originator() -> String {
            String::new()
        }
    }
}

// ─── codex-realtime-webrtc (non-Linux) ───
pub mod realtime_webrtc {
    pub struct RealtimeWebrtcSession;
    pub struct RealtimeWebrtcSessionHandle;
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
}
