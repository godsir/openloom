// Stub for codex-config types.
//
// This module provides stub types matching codex_config's public API.
// Real implementations will be added when config is ported in Phase 4.1.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod types;

// ---------------------------------------------------------------------------
// Common types and constants
// ---------------------------------------------------------------------------

/// TomlValue is a type alias for `toml::Value` (Rust 2024 allows pattern matching on type aliases).
pub type TomlValue = toml::Value;

/// Config file name constant.
pub const CONFIG_TOML_FILE: &str = "config.toml";

// ---------------------------------------------------------------------------
// ConfigLayerSource (already ported, re-export for compatibility)
// ---------------------------------------------------------------------------

pub use loom_app_server_protocol::ConfigLayerSource;

// ---------------------------------------------------------------------------
// ConfigLayerStackOrdering
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLayerStackOrdering {
    LowestPrecedenceFirst,
    HighestPrecedenceFirst,
}

// ---------------------------------------------------------------------------
// ConfigLayerEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ConfigLayerEntry {
    pub name: ConfigLayerSource,
    pub config: TomlValue,
    pub enabled: bool,
}

impl ConfigLayerEntry {
    pub fn new(name: ConfigLayerSource, config: TomlValue) -> Self {
        Self {
            name,
            config,
            enabled: true,
        }
    }

    /// Stub: returns None (no hooks config folder).
    pub fn hooks_config_folder(&self) -> Option<loom_absolute_path::AbsolutePathBuf> {
        None
    }
}

// ---------------------------------------------------------------------------
// ConfigRequirements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ConfigRequirements {
    pub allow_managed_hooks_only: Option<Constrained<bool>>,
    pub managed_hooks: Option<ManagedHooksRequirementsToml>,
}

// ---------------------------------------------------------------------------
// Constrained<T>
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Constrained<T> {
    pub value: T,
}

// ---------------------------------------------------------------------------
// ConfigRequirementsToml
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigRequirementsToml {
    pub allow_managed_hooks_only: Option<bool>,
}

// ---------------------------------------------------------------------------
// ConfigLayerStack
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ConfigLayerStack {
    layers: Vec<ConfigLayerEntry>,
    requirements: ConfigRequirements,
}

impl ConfigLayerStack {
    pub fn new(
        layers: Vec<ConfigLayerEntry>,
        _requirements: ConfigRequirementsToml,
        _defaults: ConfigRequirementsToml,
    ) -> Result<Self, String> {
        Ok(Self {
            layers,
            requirements: ConfigRequirements::default(),
        })
    }

    pub fn get_layers(
        &self,
        _ordering: ConfigLayerStackOrdering,
        include_disabled: bool,
    ) -> Vec<&ConfigLayerEntry> {
        if include_disabled {
            self.layers.iter().collect()
        } else {
            self.layers.iter().filter(|l| l.enabled).collect()
        }
    }

    pub fn effective_user_config(&self) -> Option<TomlValue> {
        self.layers
            .iter()
            .filter(|l| matches!(l.name, ConfigLayerSource::User { .. }))
            .last()
            .map(|l| l.config.clone())
    }

    pub fn effective_config(&self) -> TomlValue {
        self.layers
            .last()
            .map(|l| l.config.clone())
            .unwrap_or_else(|| TomlValue::Table(Default::default()))
    }

    pub fn requirements(&self) -> &ConfigRequirements {
        &self.requirements
    }
}

// ---------------------------------------------------------------------------
// Hook-related types
// ---------------------------------------------------------------------------

/// Hook configuration events container.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HookEventsToml {
    #[serde(flatten)]
    events: HashMap<String, Vec<MatcherGroup>>,
}

impl HookEventsToml {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn into_matcher_groups(
        self,
    ) -> Vec<(loom_protocol::protocol::HookEventName, Vec<MatcherGroup>)> {
        self.events
            .into_iter()
            .filter_map(|(k, v)| {
                let json_value = serde_json::Value::String(k);
                let event_name: loom_protocol::protocol::HookEventName =
                    serde_json::from_value(json_value).ok()?;
                Some((event_name, v))
            })
            .collect()
    }
}

/// A group of hook matchers.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatcherGroup {
    #[serde(default)]
    pub matcher: Option<String>,
    #[serde(default)]
    pub hooks: Vec<HookHandlerConfig>,
}

/// Individual hook handler configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum HookHandlerConfig {
    Command {
        #[serde(default)]
        command: String,
        #[serde(default)]
        command_windows: Option<String>,
        #[serde(default)]
        timeout_sec: Option<u64>,
        #[serde(default)]
        r#async: bool,
        #[serde(default)]
        status_message: Option<String>,
    },
    Prompt {},
    Agent {},
}

/// Hook state persisted in user config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookStateToml {
    pub enabled: Option<bool>,
    #[serde(default)]
    pub trusted_hash: Option<String>,
}

impl TryFrom<TomlValue> for HookStateToml {
    type Error = String;

    fn try_from(value: TomlValue) -> Result<Self, Self::Error> {
        let json_value = serde_json::to_value(&value).map_err(|e| e.to_string())?;
        serde_json::from_value(json_value).map_err(|e| e.to_string())
    }
}

/// The top-level hooks file format.
#[derive(Debug, Clone, Deserialize)]
pub struct HooksFile {
    pub hooks: HookEventsToml,
}

/// Managed hooks requirements from config.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ManagedHooksRequirementsToml {
    #[serde(default)]
    pub allow: Option<Vec<String>>,
    #[serde(default, skip)]
    pub source: Option<RequirementSource>,
    #[serde(default)]
    pub hooks: HookEventsToml,
}

impl ManagedHooksRequirementsToml {
    /// Stub: returns self reference.
    pub fn get(&self) -> &Self {
        self
    }

    /// Stub: always returns None.
    pub fn managed_dir_for_current_platform(&self) -> Option<loom_absolute_path::AbsolutePathBuf> {
        None
    }
}

/// Source of a config requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementSource {
    SystemRequirementsToml { file: loom_absolute_path::AbsolutePathBuf },
    LegacyManagedConfigTomlFromFile { file: loom_absolute_path::AbsolutePathBuf },
    MdmManagedPreferences { domain: String, key: String },
    CloudRequirements,
    LegacyManagedConfigTomlFromMdm,
    Unknown,
}

// ---------------------------------------------------------------------------
// Marketplace-related types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplaceConfigUpdate {
    pub marketplace_name: String,
    pub source_type: types::MarketplaceSourceType,
    pub path: Option<String>,
    pub url: Option<String>,
    pub ref_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveMarketplaceConfigOutcome {
    Removed,
    NotFound,
    Error(String),
}

// ---------------------------------------------------------------------------
// Skills-related types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillsConfig {
    #[serde(default)]
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub disabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// Plugin config types
// ---------------------------------------------------------------------------

/// Plugin-level MCP server policy.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PluginMcpServerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub default_tools_approval_mode: Option<String>,
    #[serde(default)]
    pub enabled_tools: Option<Vec<String>>,
    #[serde(default)]
    pub disabled_tools: Option<Vec<String>>,
    #[serde(default)]
    pub tools: HashMap<String, types::McpServerToolConfig>,
}

// Re-export types from types submodule
pub use types::McpServerConfig;
pub use types::McpServerToolConfig;
pub use types::PluginConfig;

// ---------------------------------------------------------------------------
// PluginConfigEdit and config editing functions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum PluginConfigEdit {
    SetEnabled {
        plugin_key: String,
        enabled: bool,
    },
    Clear {
        plugin_key: String,
    },
}

/// Apply a batch of plugin config edits to the user config file.
pub async fn apply_user_plugin_config_edits(
    _codex_home: &std::path::Path,
    _edits: Vec<PluginConfigEdit>,
) -> anyhow::Result<()> {
    Ok(())
}

/// Clear a plugin entry from user config.
pub async fn clear_user_plugin(
    _codex_home: &std::path::Path,
    _plugin_key: String,
) -> anyhow::Result<()> {
    Ok(())
}

/// Set the enabled state of a plugin in user config.
pub async fn set_user_plugin_enabled(
    _codex_home: &std::path::Path,
    _plugin_key: String,
    _enabled: bool,
) -> anyhow::Result<()> {
    Ok(())
}

/// Record a marketplace in user config.
pub fn record_user_marketplace(
    _codex_home: &std::path::Path,
    _update: MarketplaceConfigUpdate,
) -> Result<(), String> {
    Ok(())
}

/// Remove a marketplace from user config.
pub fn remove_user_marketplace_config(
    _codex_home: &std::path::Path,
    _marketplace_name: &str,
) -> Result<RemoveMarketplaceConfigOutcome, String> {
    Ok(RemoveMarketplaceConfigOutcome::Removed)
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Compute a version string for a toml value (used for cache invalidation).
pub fn version_for_toml(_config: &TomlValue) -> String {
    String::new()
}

/// Merge two toml values.
pub fn merge_toml_values(_base: TomlValue, _overlay: TomlValue) -> TomlValue {
    TomlValue::Table(Default::default())
}

/// Default project root markers.
pub fn default_project_root_markers() -> Vec<String> {
    Vec::new()
}

/// Extract project root markers from config.
pub fn project_root_markers_from_config(_config: &ConfigLayerStack) -> Vec<String> {
    Vec::new()
}
