// Loom core-plugins — minimal port for TUI compilation.
// Full implementation pending deeper type alignment.

pub const OPENAI_CURATED_MARKETPLACE_NAME: &str = "openai-curated";
pub const OPENAI_BUNDLED_MARKETPLACE_NAME: &str = "openai-bundled";

/// Stub PluginsManager
#[derive(Clone)]
pub struct PluginsManager;

impl PluginsManager {
    pub async fn new() -> Self { Self }
}
