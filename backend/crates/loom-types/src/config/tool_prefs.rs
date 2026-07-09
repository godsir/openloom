//! User-configurable built-in tool preferences.
//!
//! Persisted to `~/.loom/tool_prefs.json`.  Each field controls a knob on one of
//! the built-in tools (shell, file_read, web_search, web_fetch, process_wait,
//! monitor).

use serde::{Deserialize, Serialize};

/// Supported search backends for the built-in `web_search` tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolSearchEngine {
    DuckDuckGoLite,
    Brave,
    SearXNG,
    Google,
    Bing,
}

impl Default for ToolSearchEngine {
    fn default() -> Self {
        Self::DuckDuckGoLite
    }
}

/// User-tunable parameters for built-in tools, persisted to `~/.loom/tool_prefs.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPrefsConfig {
    // --- shell ---
    #[serde(default = "default_shell_timeout")]
    pub shell_default_timeout_secs: u64,
    #[serde(default = "default_shell_max_timeout")]
    pub shell_max_timeout_secs: u64,

    // --- file_read ---
    #[serde(default = "default_file_read_max_kb")]
    pub file_read_max_output_kb: usize,

    // --- web_search ---
    #[serde(default)]
    pub web_search_engine: ToolSearchEngine,
    #[serde(default = "default_web_search_max_results")]
    pub web_search_max_results: usize,
    /// SearXNG 自建实例地址（仅 engine=searxng 时生效）
    #[serde(default)]
    pub searxng_url: Option<String>,
    /// Google / Bing API key（engine=google 或 bing 时必填）
    #[serde(default)]
    pub web_search_api_key: Option<String>,

    // --- web_fetch ---
    #[serde(default = "default_web_fetch_max_chars")]
    pub web_fetch_max_chars: usize,

    // --- process_wait ---
    #[serde(default = "default_process_wait_max_timeout")]
    pub process_wait_max_timeout_secs: u64,

    // --- monitor ---
    #[serde(default = "default_monitor_timeout_ms")]
    pub monitor_default_timeout_ms: u64,
}

fn default_shell_timeout() -> u64 {
    60
}
fn default_shell_max_timeout() -> u64 {
    300
}
fn default_file_read_max_kb() -> usize {
    64
}
fn default_web_search_max_results() -> usize {
    5
}
fn default_web_fetch_max_chars() -> usize {
    5000
}
fn default_process_wait_max_timeout() -> u64 {
    3600
}
fn default_monitor_timeout_ms() -> u64 {
    300_000
}

impl Default for ToolPrefsConfig {
    fn default() -> Self {
        Self {
            shell_default_timeout_secs: default_shell_timeout(),
            shell_max_timeout_secs: default_shell_max_timeout(),
            file_read_max_output_kb: default_file_read_max_kb(),
            web_search_engine: ToolSearchEngine::default(),
            web_search_max_results: default_web_search_max_results(),
            searxng_url: None,
            web_search_api_key: None,
            web_fetch_max_chars: default_web_fetch_max_chars(),
            process_wait_max_timeout_secs: default_process_wait_max_timeout(),
            monitor_default_timeout_ms: default_monitor_timeout_ms(),
        }
    }
}
