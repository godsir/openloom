// loom-shim-stubs: Stub types replacing transitive cut crate dependencies.
//
// All types are minimal stubs sufficient for compilation.
// Real implementations will be added as those crates are ported.

pub use client::build_reqwest_client_with_custom_ca;

// Arg0 dispatch constants
pub const CODEX_CORE_APPLY_PATCH_ARG1: &str = "apply-patch";

// Arg0 dispatch functions
pub fn run_main() {}

/// Stub: no-op. Accepts any file system type.
pub async fn apply_patch<F: ?Sized>(
    _patch: &str,
    _cwd: &std::path::Path,
    _stdout: &mut dyn std::io::Write,
    _stderr: &mut dyn std::io::Write,
    _fs: &F,
    _sandbox: Option<&dyn std::any::Any>,
) -> Result<i32, anyhow::Error> {
    Ok(0)
}

/// Stub: no-op Unix shell escalation wrapper.
pub fn run_shell_escalation_execve_wrapper(_file: &std::path::Path, _argv: &[String]) -> ! {
    std::process::exit(1)
}

pub mod analytics;
pub mod api;
pub mod client;
pub mod code_mode;
pub mod config;
pub mod extension_api;
pub mod keyring_store;
pub mod login;
pub mod model_provider;
pub mod otel;
pub mod plugin;
pub mod utils_json_to_toml;
pub mod utils_output_truncation;
pub mod utils_pty;

// OSS provider stubs (for codex-lmstudio + codex-ollama)
pub const DEFAULT_OSS_MODEL: &str = "";

pub async fn ensure_oss_ready(_config: &dyn std::any::Any) -> anyhow::Result<()> {
    Ok(())
}

pub async fn ensure_responses_supported(_model_provider: &dyn std::any::Any) -> anyhow::Result<()> {
    Ok(())
}
