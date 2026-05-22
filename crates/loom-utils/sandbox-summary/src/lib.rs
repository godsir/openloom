// Loom sandbox-summary — minimal stub for TUI compilation.

pub fn summarize_sandbox_policy(_policy: &loom_protocol::protocol::SandboxPolicy) -> String {
    "sandbox".to_string()
}

pub fn summarize_permission_profile(
    _permission_profile: &loom_protocol::models::PermissionProfile,
    _cwd: &loom_absolute_path::AbsolutePathBuf,
    _workspace_roots: &[loom_absolute_path::AbsolutePathBuf],
) -> String {
    "workspace-write".to_string()
}

pub fn create_config_summary_entries(
    _config: &loom_tui_stubs::config::Config,
    model: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("workdir", ".".to_string()),
        ("model", model.to_string()),
        ("provider", "default".to_string()),
        ("approval", "on-request".to_string()),
        ("sandbox", "workspace-write".to_string()),
    ]
}
