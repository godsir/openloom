// Loom-chatgpt — minimal stub for TUI compilation.
// Full implementation pending deeper type alignment.

pub mod connectors {
    use loom_app_server_protocol::AppInfo;

    /// Stub: merges connector lists.
    pub fn merge_connectors_with_accessible(
        connectors: Vec<AppInfo>,
        _accessible: Vec<AppInfo>,
    ) -> Vec<AppInfo> {
        connectors
    }
}
