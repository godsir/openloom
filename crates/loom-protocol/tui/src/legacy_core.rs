//! Legacy core re-exports -- bridges the TUI crate to loom-tui-stubs.
//! Originally `codex-app-server-client::legacy_core` which re-exported from `codex-core`.

use std::path::Path;
use loom_tui_stubs::config::Config;
use loom_tui_stubs::StateDbHandle;

// Re-export stubbed types that were in codex-core
pub use loom_tui_stubs::check_execpolicy_for_warnings;
pub use loom_tui_stubs::format_exec_policy_error_with_source;
pub use loom_tui_stubs::resolve_installation_id;
pub use loom_tui_stubs::AbsolutePathBuf;
pub use loom_tui_stubs::CodexThread;
pub use loom_tui_stubs::NewThread;
pub use loom_tui_stubs::ThreadManager;
pub use loom_tui_stubs::config::Config as StubConfig;

// ─── config sub-module ───
pub mod config {
    pub use loom_tui_stubs::config::*;
    pub use loom_config::ConfigLoadError;
    pub use loom_config::ConfigLoadOptions;
    pub use loom_config::LoaderOverrides;
    pub use loom_config::format_config_error_with_source;

    pub mod edit {
        pub use loom_tui_stubs::config::edit::*;
    }
}

// ─── connectors ───
pub mod connectors {
    pub use loom_tui_stubs::connectors::*;
}

// ─── otel_init ───
pub mod otel_init {
    use loom_tui_stubs::config::Config;

    pub struct OtelProvider;

    pub fn build_provider(
        _config: &Config,
        _pkg_version: &str,
        _service_name_override: Option<&str>,
        _default_analytics_enabled: bool,
    ) -> Result<Option<OtelProvider>, anyhow::Error> {
        Ok(None)
    }

    impl OtelProvider {
        pub fn logger_layer<S>(&self) -> Option<impl tracing_subscriber::layer::Layer<S> + Send + Sync + 'static>
        where
            S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
        {
            None::<tracing_subscriber::layer::Identity>
        }

        pub fn tracing_layer<S>(&self) -> Option<impl tracing_subscriber::layer::Layer<S> + Send + Sync + 'static>
        where
            S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
        {
            None::<tracing_subscriber::layer::Identity>
        }
    }

    pub fn record_process_start(_otel: Option<&OtelProvider>, _originator: &str) {}
    pub fn install_sqlite_telemetry(_otel: Option<&OtelProvider>, _originator: &str) {}
}

// ─── personality_migration ───
pub mod personality_migration {
    use std::path::Path;
    use loom_tui_stubs::config::Config;
    use loom_tui_stubs::StateDbHandle;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PersonalityMigrationStatus {
        Applied,
        SkippedMarker,
        SkippedExplicitPersonality,
        SkippedNoSessions,
    }

    pub async fn maybe_migrate_personality(
        _codex_home: &Path,
        _config_toml: &(),
        _state_db: Option<StateDbHandle>,
    ) -> Result<PersonalityMigrationStatus, anyhow::Error> {
        Ok(PersonalityMigrationStatus::SkippedNoSessions)
    }
}

// ─── windows_sandbox ───
pub mod windows_sandbox {
    use std::path::Path;

    #[derive(Debug, Clone)]
    pub struct WindowsSandboxLevelExt;

    pub const ELEVATED_SANDBOX_NUX_ENABLED: bool = false;

    pub fn apply_world_writable_scan_and_denies() {}
    pub fn elevated_setup_failure_details() -> String { String::new() }
    pub fn elevated_setup_failure_metric_name() -> &'static str { "elevated_setup_failure" }
    pub fn run_elevated_setup() {}
    pub fn run_legacy_setup_preflight(_codex_home: &Path) -> Result<(), anyhow::Error> { Ok(()) }
    pub fn sandbox_setup_is_complete(_codex_home: &Path) -> bool { true }
    pub fn grant_read_root_non_elevated() {}
}

// ─── util ───
pub mod util {
    pub fn normalize_thread_name(name: &str) -> String { name.to_string() }
}

// ─── Other stub modules ───
pub mod review_format {}
pub mod review_prompts {}
pub mod test_support {}

// ─── state_db functions (re-export from tui-stubs) ───
pub use loom_tui_stubs::rollout::state_db;

// ─── Constants ───
pub const DEFAULT_AGENTS_MD_FILENAME: &str = "AGENTS.md";
pub const LOCAL_AGENTS_MD_FILENAME: &str = "AGENTS.local.md";
pub const CODEX_CLI_VERSION: &str = "0.2.0";

// ─── McpManager stub ───
pub struct McpManager;
impl McpManager {
    pub async fn new() -> Self { Self }
}

// ─── web_search_detail stub ───
pub fn web_search_detail() -> String { String::new() }
