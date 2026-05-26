//! Application configuration types.
//!
//! Consumers: loom-core (engine config), loom-server (config.* RPC methods), loom-cli (config command)

pub mod model_config;

use serde::{Deserialize, Serialize};

use model_config::ModelConfig;

// === Sub-configs ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPrefs {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_max_iterations() -> usize { 20 }
fn default_timeout_secs() -> u64 { 300 }

impl Default for AgentPrefs {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaPrefs {
    #[serde(default = "default_top_n")]
    pub top_n: usize,
    #[serde(default = "default_recency_decay")]
    pub recency_decay_days: u32,
}

fn default_top_n() -> usize { 5 }
fn default_recency_decay() -> u32 { 30 }

impl Default for PersonaPrefs {
    fn default() -> Self {
        Self {
            top_n: default_top_n(),
            recency_decay_days: default_recency_decay(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouterPrefs {
    #[serde(default = "default_keyword_threshold")]
    pub keyword_threshold: f32,
    #[serde(default = "default_fallback_threshold")]
    pub fallback_threshold: f32,
}

fn default_keyword_threshold() -> f32 { 0.85 }
fn default_fallback_threshold() -> f32 { 0.70 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerPrefs {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 0 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoragePrefs {
    pub data_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoggingPrefs {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub log_content: bool,
}

fn default_log_level() -> String { "info".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePrefs {
    #[serde(default = "default_block_size")]
    pub block_size: usize,
    #[serde(default = "default_max_blocks")]
    pub max_blocks: usize,
    #[serde(default = "default_cache_budget_mb")]
    pub total_budget_mb: usize,
}

fn default_block_size() -> usize { 1024 }
fn default_max_blocks() -> usize { 32 }
fn default_cache_budget_mb() -> usize { 256 }

impl Default for CachePrefs {
    fn default() -> Self {
        Self {
            block_size: default_block_size(),
            max_blocks: default_max_blocks(),
            total_budget_mb: default_cache_budget_mb(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimitConfig {
    #[serde(default = "default_min_interval")]
    pub min_interval_ms: u64,
}

fn default_min_interval() -> u64 { 100 }

// === Root config ===

/// Top-level application configuration.
///
/// Consumers: loom-core (Engine::get_config/set_config), loom-server (config.* RPC), loom-cli
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub models: Vec<ModelConfig>,
    #[serde(default)]
    pub router: RouterPrefs,
    #[serde(default)]
    pub server: ServerPrefs,
    #[serde(default)]
    pub storage: StoragePrefs,
    #[serde(default)]
    pub logging: LoggingPrefs,
    #[serde(default)]
    pub cache: CachePrefs,
    #[serde(default)]
    pub agent: AgentPrefs,
    #[serde(default)]
    pub persona: PersonaPrefs,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    /// Free-form settings blob for backwards compatibility with Electron settings UI.
    #[serde(default)]
    pub settings: serde_json::Value,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            models: vec![ModelConfig::default()],
            router: RouterPrefs::default(),
            server: ServerPrefs::default(),
            storage: StoragePrefs::default(),
            logging: LoggingPrefs::default(),
            cache: CachePrefs::default(),
            agent: AgentPrefs::default(),
            persona: PersonaPrefs::default(),
            rate_limit: RateLimitConfig::default(),
            settings: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

/// Agent configuration stored in settings.agent.{id}.config.
///
/// Consumers: loom-core (AgentPool::spawn), loom-server (agent.configure RPC)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
    /// Natural-language persona description.
    #[serde(default)]
    pub persona: String,
    #[serde(default)]
    pub system_prompt_override: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub thinking_level: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub tool_scope: Option<String>,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub disallowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub max_iterations: Option<usize>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default = "default_max_subagents")]
    pub max_concurrent_subagents: usize,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(default)]
    pub memory_enabled: bool,
}

fn default_max_subagents() -> usize { 5 }
