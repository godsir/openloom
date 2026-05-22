// Stub for codex-plugin types.

use loom_absolute_path::AbsolutePathBuf;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

// ---------------------------------------------------------------------------
// PluginId
// ---------------------------------------------------------------------------

/// Plugin identity key: `<plugin_name>@<marketplace_name>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginId {
    pub plugin_name: String,
    pub marketplace_name: String,
}

impl PluginId {
    /// Create a new PluginId from plugin and marketplace names.
    pub fn new(
        plugin_name: String,
        marketplace_name: String,
    ) -> Result<Self, PluginIdError> {
        let plugin_name = plugin_name.trim().to_string();
        if plugin_name.is_empty() {
            return Err(PluginIdError::Invalid("plugin name is empty".to_string()));
        }
        let marketplace_name = marketplace_name.trim().to_string();
        if marketplace_name.is_empty() {
            return Err(PluginIdError::Invalid(
                "marketplace name is empty".to_string(),
            ));
        }
        if plugin_name.contains('@') {
            return Err(PluginIdError::Invalid(
                "plugin name contains '@'".to_string(),
            ));
        }
        Ok(Self {
            plugin_name,
            marketplace_name,
        })
    }

    /// Parse a plugin key in the form `<plugin_name>@<marketplace_name>`.
    pub fn parse(key: &str) -> Result<Self, PluginIdError> {
        let key = key.trim();
        let (plugin_name, marketplace_name) = key
            .rsplit_once('@')
            .ok_or_else(|| PluginIdError::Invalid(format!("missing '@' in plugin key: {key}")))?;
        Self::new(plugin_name.to_string(), marketplace_name.to_string())
    }

    /// Return the canonical string key `<plugin_name>@<marketplace_name>`.
    pub fn as_key(&self) -> String {
        format!("{}@{}", self.plugin_name, self.marketplace_name)
    }
}

// ---------------------------------------------------------------------------
// PluginIdError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginIdError {
    Invalid(String),
}

impl fmt::Display for PluginIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(msg) => write!(f, "invalid plugin id: {msg}"),
        }
    }
}

impl std::error::Error for PluginIdError {}

// ---------------------------------------------------------------------------
// AppConnectorId
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppConnectorId(pub String);

// ---------------------------------------------------------------------------
// PluginCapabilitySummary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginCapabilitySummary {
    pub config_name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub has_skills: bool,
    pub mcp_server_names: Vec<String>,
    pub app_connector_ids: Vec<AppConnectorId>,
}

// ---------------------------------------------------------------------------
// PluginHookSource
// ---------------------------------------------------------------------------

/// Plugin hook source with embedded hook definitions.
#[derive(Debug, Clone)]
pub struct PluginHookSource {
    pub plugin_id: PluginId,
    pub plugin_root: AbsolutePathBuf,
    pub plugin_data_root: AbsolutePathBuf,
    pub source_path: AbsolutePathBuf,
    pub source_relative_path: String,
    pub hooks: crate::config::HookEventsToml,
}

// ---------------------------------------------------------------------------
// LoadedPlugin<C>
// ---------------------------------------------------------------------------

/// A plugin loaded from disk with its resolved capabilities.
#[derive(Debug, Clone)]
pub struct LoadedPlugin<C> {
    pub config_name: String,
    pub manifest_name: Option<String>,
    pub manifest_description: Option<String>,
    pub root: AbsolutePathBuf,
    pub enabled: bool,
    pub skill_roots: Vec<AbsolutePathBuf>,
    pub disabled_skill_paths: HashSet<AbsolutePathBuf>,
    pub has_enabled_skills: bool,
    pub mcp_servers: HashMap<String, C>,
    pub apps: Vec<AppConnectorId>,
    pub hook_sources: Vec<PluginHookSource>,
    pub hook_load_warnings: Vec<String>,
    pub error: Option<String>,
}

// Stub PluginSkillRoot (avoids depending on loom-plugins which isn't workspace-ready yet).

/// Stub replacement for loom_plugins::PluginSkillRoot.
#[derive(Debug, Clone)]
pub struct PluginSkillRoot {
    pub plugin_id: Option<String>,
    pub root: AbsolutePathBuf,
    pub skill_path: AbsolutePathBuf,
}

// ---------------------------------------------------------------------------
// PluginLoadOutcome<C>
// ---------------------------------------------------------------------------

/// The result of loading all configured plugins.
#[derive(Debug, Clone, Default)]
pub struct PluginLoadOutcome<C> {
    plugins: Vec<LoadedPlugin<C>>,
}

impl<C> PluginLoadOutcome<C> {
    pub fn from_plugins(plugins: Vec<LoadedPlugin<C>>) -> Self {
        Self { plugins }
    }

    pub fn plugins(&self) -> &[LoadedPlugin<C>] {
        &self.plugins
    }

    pub fn effective_plugin_skill_roots(&self) -> Vec<PluginSkillRoot> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// PluginTelemetryMetadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PluginTelemetryMetadata {
    pub plugin_id: PluginId,
    pub remote_plugin_id: Option<String>,
    pub capability_summary: Option<PluginCapabilitySummary>,
}

impl PluginTelemetryMetadata {
    pub fn from_plugin_id(plugin_id: &PluginId) -> Self {
        Self {
            plugin_id: plugin_id.clone(),
            remote_plugin_id: None,
            capability_summary: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Validate a plugin segment name (must be non-empty, no whitespace, no '@').
pub fn validate_plugin_segment(segment: &str) -> Result<(), String> {
    if segment.trim().is_empty() {
        return Err("plugin segment is empty".to_string());
    }
    if segment.contains(char::is_whitespace) || segment.contains('@') {
        return Err(format!("invalid plugin segment: {segment}"));
    }
    Ok(())
}

/// Return a prompt-safe description or None.
pub fn prompt_safe_plugin_description(description: Option<&str>) -> Option<String> {
    description.map(|s| s.to_string())
}
