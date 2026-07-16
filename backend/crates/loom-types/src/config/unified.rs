//! Unified configuration - replaces the scattered JSON files in ~/.loom.
//!
//! Before: 8 separate files (preferences.json, mcp.json, tool_prefs.json,
//! vision.json, auxiliary.json, fim.json, sandbox.json, workspace.json)
//! Now:   single ~/.loom/config.json with typed sections.
//!
//! `ConfigStore` is the single entry point for all config I/O.  It handles:
//! - Atomic read-modify-write via an in-process RwLock
//! - Automatic migration from legacy per-file format
//! - Backwards-compatible section accessors

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::sync::RwLock;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::SandboxConfig;
use super::tool_prefs::ToolPrefsConfig;

// ============================================================
// MCP section
// ============================================================

/// MCP server configuration entry - mirrors the legacy `mcp.json` format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    #[serde(rename = "type", default = "default_mcp_type")]
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_mcp_type() -> String {
    "stdio".into()
}

/// MCP section of the unified config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfigSection {
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerEntry>,
}

// ============================================================
// Vision section
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VisionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub model: Option<String>,
}

// ============================================================
// FIM section
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FimConfig {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

// ============================================================
// Auxiliary section
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuxiliaryConfig {
    #[serde(default)]
    pub summary_model: Option<String>,
    #[serde(default)]
    pub entity_model: Option<String>,
}

// ============================================================
// Workspace section
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub default_workspace: Option<String>,
}

// ============================================================
// Unified config root
// ============================================================

/// Top-level unified configuration stored at `~/.loom/config.json`.
///
/// Each section replaces a former standalone JSON file.  The `preferences`
/// section is kept as a `serde_json::Value` because the Electron frontend
/// owns its schema and reads/writes it freely via `store.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedConfig {
    /// Schema version for future migrations.  Currently 1.
    #[serde(default = "default_config_version")]
    pub version: u32,

    /// UI preferences (formerly `preferences.json`).
    /// Owned by the Electron frontend - stored as a free-form JSON value.
    #[serde(default)]
    pub preferences: serde_json::Value,

    /// MCP server configs (formerly `mcp.json`).
    #[serde(default)]
    pub mcp: McpConfigSection,

    /// Built-in tool tunables (formerly `tool_prefs.json`).
    #[serde(default)]
    pub tool_prefs: ToolPrefsConfig,

    /// Vision auxiliary model config (formerly `vision.json`).
    #[serde(default)]
    pub vision: VisionConfig,

    /// Summary/entity extraction model config (formerly `auxiliary.json`).
    #[serde(default)]
    pub auxiliary: AuxiliaryConfig,

    /// Fill-in-the-middle config (formerly `fim.json`).
    #[serde(default)]
    pub fim: FimConfig,

    /// File-system sandbox rules (formerly `sandbox.json`).
    #[serde(default)]
    pub sandbox: SandboxConfig,

    /// Default workspace path (formerly `workspace.json`).
    #[serde(default)]
    pub workspace: WorkspaceConfig,
}

fn default_config_version() -> u32 {
    1
}

impl Default for UnifiedConfig {
    fn default() -> Self {
        Self {
            version: default_config_version(),
            preferences: serde_json::Value::Object(serde_json::Map::new()),
            mcp: McpConfigSection::default(),
            tool_prefs: ToolPrefsConfig::default(),
            vision: VisionConfig::default(),
            auxiliary: AuxiliaryConfig::default(),
            fim: FimConfig::default(),
            sandbox: SandboxConfig::default(),
            workspace: WorkspaceConfig::default(),
        }
    }
}

// ============================================================
// ConfigStore - single entry point for config I/O
// ============================================================

/// Thread-safe unified configuration store.
///
/// Wraps a `UnifiedConfig` in an `RwLock` so all callers share one
/// in-memory copy.  Writes are persisted atomically to
/// `~/.loom/config.json`.
///
/// On first load, if `config.json` does not exist but legacy per-file
/// configs do, they are migrated into the unified file.
pub struct ConfigStore {
    config: Arc<RwLock<UnifiedConfig>>,
    path: PathBuf,
}

impl ConfigStore {
    /// Path to the unified config file inside the given data dir.
    pub fn config_path(data_dir: &Path) -> PathBuf {
        data_dir.join("config.json")
    }

    /// Load from `data_dir/config.json`, migrating legacy files if needed.
    pub fn load(data_dir: &Path) -> Result<Self> {
        let path = Self::config_path(data_dir);

        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let mut config: UnifiedConfig = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            // Ensure version is set for hand-edited configs missing the field.
            if config.version == 0 {
                config.version = default_config_version();
            }
            Ok(Self {
                config: Arc::new(RwLock::new(config)),
                path,
            })
        } else {
            // Migrate from legacy per-file configs.
            let config = Self::migrate_from_legacy(data_dir);
            // Persist the migrated config before wrapping in the lock.
            let path_for_persist = path.clone();
            let persist_ok = Self::persist_to(&path_for_persist, &config).is_ok();
            // Now that the unified config is safely on disk, remove the
            // legacy per-file configs so there is a single source of truth
            // (later edits can't diverge between an old file and config.json).
            // `mcp.json` is intentionally kept — still read by `loom mcp list`
            // and as a project-level import source.
            if persist_ok {
                Self::remove_legacy_files(data_dir);
            }
            let store = Self {
                config: Arc::new(RwLock::new(config)),
                path,
            };
            Ok(store)
        }
    }

    /// Load from `data_dir/config.json`, or return a default store if loading fails.
    /// Never panics — used in orchestrator construction where failure is not an option.
    pub fn load_or_default(data_dir: &Path) -> Self {
        match Self::load(data_dir) {
            Ok(store) => store,
            Err(e) => {
                tracing::warn!(error = %e, "config store load failed, using defaults");
                let path = Self::config_path(data_dir);
                Self {
                    config: Arc::new(RwLock::new(UnifiedConfig::default())),
                    path,
                }
            }
        }
    }

    /// Synchronously get a snapshot of the full config.
    /// Use only in non-async contexts (e.g. during construction).
    pub fn blocking_get(&self) -> UnifiedConfig {
        self.config.read().unwrap().clone()
    }

    /// Read the config path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get a snapshot of the full config.
    pub async fn get(&self) -> UnifiedConfig {
        self.config.read().unwrap().clone()
    }

    /// Replace the full config and persist.
    pub async fn save(&self, config: UnifiedConfig) -> Result<()> {
        self.persist(&config)?;
        *self.config.write().unwrap() = config;
        Ok(())
    }

    // --- Typed section accessors ---

    /// Read the MCP section.
    pub async fn mcp(&self) -> McpConfigSection {
        self.config.read().unwrap().mcp.clone()
    }

    /// Read the tool_prefs section.
    pub async fn tool_prefs(&self) -> ToolPrefsConfig {
        self.config.read().unwrap().tool_prefs.clone()
    }

    /// Read the vision section.
    pub async fn vision(&self) -> VisionConfig {
        self.config.read().unwrap().vision.clone()
    }

    /// Read the auxiliary section.
    pub async fn auxiliary(&self) -> AuxiliaryConfig {
        self.config.read().unwrap().auxiliary.clone()
    }

    /// Read the FIM section.
    pub async fn fim(&self) -> FimConfig {
        self.config.read().unwrap().fim.clone()
    }

    /// Read the sandbox section.
    pub async fn sandbox(&self) -> SandboxConfig {
        self.config.read().unwrap().sandbox.clone()
    }

    /// Read the workspace section.
    pub async fn workspace(&self) -> WorkspaceConfig {
        self.config.read().unwrap().workspace.clone()
    }

    /// Read the preferences section (free-form JSON).
    pub async fn preferences(&self) -> serde_json::Value {
        self.config.read().unwrap().preferences.clone()
    }

    // --- Section writers (read-modify-write under lock, then persist) ---

    /// Update the MCP section.
    pub async fn save_mcp(&self, mcp: McpConfigSection) -> Result<()> {
        let mut guard = self.config.write().unwrap();
        guard.mcp = mcp;
        self.persist(&guard)
    }

    /// Update the tool_prefs section.
    pub async fn save_tool_prefs(&self, prefs: ToolPrefsConfig) -> Result<()> {
        let mut guard = self.config.write().unwrap();
        guard.tool_prefs = prefs;
        self.persist(&guard)
    }

    /// Update the vision section.
    pub async fn save_vision(&self, vision: VisionConfig) -> Result<()> {
        let mut guard = self.config.write().unwrap();
        guard.vision = vision;
        self.persist(&guard)
    }

    /// Update the auxiliary section.
    pub async fn save_auxiliary(&self, aux: AuxiliaryConfig) -> Result<()> {
        let mut guard = self.config.write().unwrap();
        guard.auxiliary = aux;
        self.persist(&guard)
    }

    /// Update the FIM section.
    pub async fn save_fim(&self, fim: FimConfig) -> Result<()> {
        let mut guard = self.config.write().unwrap();
        guard.fim = fim;
        self.persist(&guard)
    }

    /// Update the sandbox section.
    pub async fn save_sandbox(&self, sandbox: SandboxConfig) -> Result<()> {
        let mut guard = self.config.write().unwrap();
        guard.sandbox = sandbox;
        self.persist(&guard)
    }

    /// Update the workspace section.
    pub async fn save_workspace(&self, ws: WorkspaceConfig) -> Result<()> {
        let mut guard = self.config.write().unwrap();
        guard.workspace = ws;
        self.persist(&guard)
    }

    /// Update the preferences section.
    pub async fn save_preferences(&self, prefs: serde_json::Value) -> Result<()> {
        let mut guard = self.config.write().unwrap();
        guard.preferences = prefs;
        self.persist(&guard)
    }

    // --- Internals ---

    /// Write config to a specific path (static, no lock needed).
    fn persist_to(path: &Path, config: &UnifiedConfig) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config dir: {}", parent.display())
            })?;
        }
        let json = serde_json::to_string_pretty(config)
            .context("failed to serialize config.json")?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json)
            .with_context(|| format!("failed to write {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("failed to rename to {}", path.display()))?;
        Ok(())
    }

    /// Write config to disk.  Caller must already hold the write lock
    /// (or operate on a snapshot that is then stored).
    fn persist(&self, config: &UnifiedConfig) -> Result<()> {
        Self::persist_to(&self.path, config)
    }

    /// Migrate legacy per-file configs into a single `UnifiedConfig`.
    ///
    /// Each legacy file is read best-effort - missing or unparseable
    /// files simply yield their default section.
    fn migrate_from_legacy(data_dir: &Path) -> UnifiedConfig {
        let mut config = UnifiedConfig::default();

        // preferences.json - free-form JSON object
        if let Ok(content) = std::fs::read_to_string(data_dir.join("preferences.json"))
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                config.preferences = val;
            }

        // mcp.json - { "mcpServers": { ... } }
        if let Ok(content) = std::fs::read_to_string(data_dir.join("mcp.json"))
            && let Ok(mcp) = serde_json::from_str::<McpConfigSection>(&content) {
                config.mcp = mcp;
            }

        // tool_prefs.json
        if let Ok(content) = std::fs::read_to_string(data_dir.join("tool_prefs.json"))
            && let Ok(prefs) = serde_json::from_str::<ToolPrefsConfig>(&content) {
                config.tool_prefs = prefs;
            }

        // vision.json
        if let Ok(content) = std::fs::read_to_string(data_dir.join("vision.json"))
            && let Ok(vision) = serde_json::from_str::<VisionConfig>(&content) {
                config.vision = vision;
            }

        // auxiliary.json - { "summary_model": ..., "entity_model": ... }
        if let Ok(content) = std::fs::read_to_string(data_dir.join("auxiliary.json"))
            && let Ok(aux) = serde_json::from_str::<AuxiliaryConfig>(&content) {
                config.auxiliary = aux;
            }

        // fim.json
        if let Ok(content) = std::fs::read_to_string(data_dir.join("fim.json"))
            && let Ok(fim) = serde_json::from_str::<FimConfig>(&content) {
                config.fim = fim;
            }

        // sandbox.json
        if let Ok(content) = std::fs::read_to_string(data_dir.join("sandbox.json"))
            && let Ok(sb) = serde_json::from_str::<SandboxConfig>(&content) {
                config.sandbox = sb;
            }

        // workspace.json - { "default_workspace": "..." }
        if let Ok(content) = std::fs::read_to_string(data_dir.join("workspace.json"))
            && let Ok(ws) = serde_json::from_str::<WorkspaceConfig>(&content) {
                config.workspace = ws;
            }

        tracing::info!("migrated legacy config files into config.json");

        // If no MCP servers were found in any legacy file, seed defaults
        // (playwright + context7) so the user has something to start with.
        if config.mcp.mcp_servers.is_empty() {
            let mut servers = HashMap::new();
            servers.insert(
                "playwright".into(),
                McpServerEntry {
                    transport: "stdio".into(),
                    command: "npx".into(),
                    args: vec!["-y".into(), "@playwright/mcp@latest".into()],
                    url: None,
                    headers: HashMap::new(),
                    env: HashMap::new(),
                },
            );
            servers.insert(
                "context7".into(),
                McpServerEntry {
                    transport: "stdio".into(),
                    command: "npx".into(),
                    args: vec!["-y".into(), "@upstash/context7-mcp".into()],
                    url: None,
                    headers: HashMap::new(),
                    env: HashMap::new(),
                },
            );
            config.mcp = McpConfigSection { mcp_servers: servers };
        }

        config
    }

    /// Remove legacy per-file configs after a successful migration.
    ///
    /// Only the standalone files whose data now lives in config.json are
    /// removed — this keeps a single source of truth so later edits can't
    /// diverge between the old file and the unified config. Called only
    /// after the unified config has been persisted successfully.
    ///
    /// `mcp.json` is intentionally NOT removed: it is still read by the CLI
    /// (`loom mcp list` / project-level `.loom/mcp.json` import) and is a
    /// user-facing import source rather than a migrated setting.
    fn remove_legacy_files(data_dir: &Path) {
        for name in [
            "preferences.json",
            "tool_prefs.json",
            "vision.json",
            "auxiliary.json",
            "fim.json",
            "sandbox.json",
            "workspace.json",
        ] {
            let p = data_dir.join(name);
            if p.exists() {
                match std::fs::remove_file(&p) {
                    Ok(()) => tracing::info!(
                        path = %p.display(),
                        "removed legacy config after migration"
                    ),
                    Err(e) => tracing::warn!(
                        path = %p.display(),
                        error = %e,
                        "failed to remove legacy config"
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = UnifiedConfig::default();
        assert_eq!(config.version, 1);
        assert!(config.preferences.is_object());
        assert!(config.mcp.mcp_servers.is_empty());
    }

    #[tokio::test]
    async fn test_roundtrip() {
        let dir = tempdir().unwrap();
        let store = ConfigStore::load(dir.path()).unwrap();

        let mut mcp = McpConfigSection::default();
        mcp.mcp_servers.insert(
            "test".into(),
            McpServerEntry {
                transport: "stdio".into(),
                command: "npx".into(),
                args: vec!["-y".into(), "@test/mcp".into()],
                url: None,
                headers: HashMap::new(),
                env: HashMap::new(),
            },
        );

        store.save_mcp(mcp.clone()).await.unwrap();
        assert_eq!(store.mcp().await.mcp_servers.len(), 1);

        // Reload from disk
        let store2 = ConfigStore::load(dir.path()).unwrap();
        assert_eq!(store2.mcp().await.mcp_servers.len(), 1);
        let binding = store2.mcp().await;
        let entry = binding.mcp_servers.get("test").unwrap();
        assert_eq!(entry.command, "npx");
    }

    #[tokio::test]
    async fn test_migrate_from_legacy() {
        let dir = tempdir().unwrap();

        // Write a legacy vision.json
        let vision_json = r#"{"enabled": true, "model": "qwen3-vl-plus"}"#;
        std::fs::write(dir.path().join("vision.json"), vision_json).unwrap();

        // Write a legacy workspace.json
        let ws_json = r#"{"default_workspace": "F:/work"}"#;
        std::fs::write(dir.path().join("workspace.json"), ws_json).unwrap();

        // Write a legacy mcp.json
        let mcp_json = r#"{"mcpServers": {"foo": {"type": "stdio", "command": "bar"}}}"#;
        std::fs::write(dir.path().join("mcp.json"), mcp_json).unwrap();

        let store = ConfigStore::load(dir.path()).unwrap();
        let config = store.get().await;

        assert_eq!(config.version, 1);
        assert!(config.vision.enabled);
        assert_eq!(config.vision.model.as_deref(), Some("qwen3-vl-plus"));
        assert_eq!(
            config.workspace.default_workspace.as_deref(),
            Some("F:/work")
        );
        assert!(config.mcp.mcp_servers.contains_key("foo"));

        // config.json should now exist on disk
        assert!(dir.path().join("config.json").exists());
    }

    #[tokio::test]
    async fn test_section_update_persists() {
        let dir = tempdir().unwrap();
        let store = ConfigStore::load(dir.path()).unwrap();

        let fim = FimConfig {
            model: Some("deepseek-v4-pro-fim".into()),
            base_url: None,
            api_key_env: None,
        };
        store.save_fim(fim).await.unwrap();

        let store2 = ConfigStore::load(dir.path()).unwrap();
        assert_eq!(
            store2.fim().await.model.as_deref(),
            Some("deepseek-v4-pro-fim")
        );
    }
}
