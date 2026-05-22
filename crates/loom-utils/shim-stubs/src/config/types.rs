// Config types submodule (codex_config::types).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MCP server configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub default_tools_approval_mode: Option<String>,
    #[serde(default)]
    pub enabled_tools: Option<Vec<String>>,
    #[serde(default)]
    pub disabled_tools: Option<Vec<String>>,
    #[serde(default)]
    pub tools: HashMap<String, McpServerToolConfig>,
}

/// Per-tool MCP server configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct McpServerToolConfig {
    #[serde(default)]
    pub approval_mode: Option<String>,
}

/// MCP server transport configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerTransportConfig {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// MCP server OAuth configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct McpServerOAuthConfig {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub authorization_url: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
}

/// Plugin user configuration entry.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mcp_servers: HashMap<String, super::PluginMcpServerConfig>,
}

/// Marketplace configuration entry.
#[derive(Debug, Clone, Deserialize)]
pub struct MarketplaceConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source: Option<MarketplaceSourceType>,
}

/// How a marketplace source is configured.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum MarketplaceSourceType {
    #[serde(rename = "git")]
    Git,
    #[serde(rename = "local")]
    Local,
}

impl Default for MarketplaceSourceType {
    fn default() -> Self {
        Self::Local
    }
}
