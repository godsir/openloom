// Loom core-plugins — minimal port for TUI compilation.
// Full implementation pending deeper type alignment.

use std::path::PathBuf;

pub const OPENAI_CURATED_MARKETPLACE_NAME: &str = "openai-curated";
pub const OPENAI_BUNDLED_MARKETPLACE_NAME: &str = "openai-bundled";

/// Stub PluginsManager
#[derive(Clone)]
pub struct PluginsManager;

impl PluginsManager {
    pub fn new(_codex_home: PathBuf) -> Self { Self }
    pub fn plugins_for_config(&self, _plugins_input: &()) -> &Self { self }
}
