// Plugin filesystem management for openLoom.
//
// Universal plugin support: scans for multiple manifest formats
// (.loom-plugin, .claude-plugin, .codex-plugin, manifest.json).
//
// Manages install/remove/enable/disable and marketplace source scanning.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Manifest types ──

/// OpenLoom/Hanako manifest.json format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoomManifest {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub trust: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, alias = "minAppVersion")]
    pub min_app_version: Option<String>,
    #[serde(default)]
    pub ui: Option<PluginUiConfig>,
    #[serde(default)]
    pub contributes: Option<PluginContributes>,
}

/// Universal plugin manifest (.loom-plugin/.claude-plugin/.codex-plugin/plugin.json).
/// These share a common schema across tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub skills: Option<UniversalSkillsRef>,
    #[serde(default, alias = "mcpServers")]
    pub mcp_servers: Option<UniversalMcpRef>,
    #[serde(default)]
    pub apps: Option<UniversalAppsRef>,
    #[serde(default)]
    pub hooks: Option<serde_json::Value>,
    #[serde(default)]
    pub interface: Option<UniversalInterface>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalSkillsRef {
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalMcpRef {
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalAppsRef {
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalInterface {
    #[serde(default, alias = "displayName")]
    pub display_name: Option<String>,
    #[serde(default, alias = "shortDescription")]
    pub short_description: Option<String>,
    #[serde(default, alias = "developerName")]
    pub developer_name: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default, alias = "brandColor")]
    pub brand_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUiConfig {
    #[serde(default, alias = "hostCapabilities")]
    pub host_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginContributes {
    #[serde(default)]
    pub configuration: Option<serde_json::Value>,
}

// ── Output types ──

#[derive(Debug, Clone, Serialize)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: String,
    pub status: String,
    pub trust: String,
    pub hidden: bool,
    pub plugin_dir: String,
    pub manifest_format: String, // "loom", "loom-plugin", "claude-plugin", "codex-plugin"
    pub contributions: Vec<String>,
    pub config_schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_host_capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Marketplace plugin entry (returned to frontend).
#[derive(Debug, Clone, Serialize)]
pub struct MarketplacePlugin {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub trust: String,
    #[serde(default)]
    pub contributions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    pub compatibility: MarketplaceCompatibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distribution: Option<MarketplaceDistribution>,
    // Computed fields (vs installed)
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub can_install: bool,
    pub install_action: String, // "install" | "update" | "incompatible"
    pub compatible: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceCompatibility {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceDistribution {
    pub kind: String, // "source"
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceSource {
    pub kind: String,  // "git", "local"
    pub url: Option<String>,
    pub path: Option<String>,
    pub configured: bool,
    pub name: String,
}

// ── marketplace.json parsing ──

/// Root marketplace listing manifest (.claude-plugin/marketplace.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceListing {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub owner: Option<MarketplaceOwner>,
    #[serde(default)]
    pub plugins: Vec<MarketplaceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceOwner {
    #[serde(default)]
    pub name: String,
}

/// A single plugin entry in marketplace.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<MarketplaceOwner>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    /// Source can be a relative path string ("./plugins/xxx") or an object.
    #[serde(default)]
    pub source: Option<serde_json::Value>,
}

/// Parse a marketplace.json file and convert entries to MarketplacePlugins.
pub fn parse_marketplace_listing(
    listing_path: &Path,
    repo_root: &Path,
) -> Result<Vec<MarketplacePlugin>> {
    let content = std::fs::read_to_string(listing_path)?;
    let listing: MarketplaceListing = serde_json::from_str(&content)?;

    let mut plugins = Vec::new();
    for entry in &listing.plugins {
        let (local_path, is_remote, remote_url) = parse_source(&entry.source, repo_root);

        let id = slugify(&entry.name);
        let mut mp = MarketplacePlugin {
            id: id.clone(),
            name: entry.name.clone(),
            publisher: entry
                .author
                .as_ref()
                .map(|a| a.name.clone())
                .or_else(|| listing.owner.as_ref().map(|o| o.name.clone())),
            version: entry.version.clone(),
            description: if entry.description.is_empty() { None } else { Some(entry.description.clone()) },
            trust: "restricted".to_string(),
            contributions: Vec::new(),
            repository: remote_url,
            compatibility: MarketplaceCompatibility { min_app_version: None },
            distribution: local_path.as_ref().map(|p| MarketplaceDistribution {
                kind: "source".to_string(),
                path: p.to_string_lossy().to_string(),
            }),
            installed: false,
            installed_version: None,
            latest_version: entry.version.clone(),
            update_available: false,
            can_install: !is_remote,
            install_action: if is_remote { "incompatible".to_string() } else { "install".to_string() },
            compatible: !is_remote,
        };

        // If local, try to load the actual plugin manifest for richer metadata
        if let Some(ref lp) = local_path {
            if let Ok(plugin_entry) = load_plugin(lp) {
                let ver = plugin_entry.version.clone();
                mp.version = Some(ver.clone());
                mp.latest_version = Some(ver);
                mp.contributions = plugin_entry.contributions;
                mp.trust = plugin_entry.trust;
                if mp.description.is_none() && !plugin_entry.description.is_empty() {
                    mp.description = Some(plugin_entry.description);
                }
                if mp.publisher.is_none() {
                    mp.publisher = plugin_entry.author;
                }
                mp.repository = mp.repository.or(plugin_entry.repository);
            }
        }

        plugins.push(mp);
    }
    Ok(plugins)
}

/// Parse the source field of a marketplace entry.
/// Returns (local_path, is_remote, remote_url).
fn parse_source(
    source: &Option<serde_json::Value>,
    repo_root: &Path,
) -> (Option<std::path::PathBuf>, bool, Option<String>) {
    match source {
        None => (None, true, None),
        Some(serde_json::Value::String(s)) => {
            if s.starts_with("./") || s.starts_with("../") {
                let local = repo_root.join(s);
                (Some(local), false, None)
            } else if s.starts_with("http") {
                (None, true, Some(s.clone()))
            } else {
                (None, true, None)
            }
        }
        Some(obj) => {
            let source_type = obj.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let url = obj.get("url").and_then(|v| v.as_str()).map(String::from);
            match source_type {
                "git-subdir" | "url" => {
                    // External remote plugin — not locally available
                    (None, true, url)
                }
                _ => (None, true, url),
            }
        }
    }
}

// ── Manifest detection order ──

const MANIFEST_CANDIDATES: &[(&str, &str)] = &[
    (".loom-plugin/plugin.json", "loom-plugin"),
    (".claude-plugin/plugin.json", "claude-plugin"),
    (".codex-plugin/plugin.json", "codex-plugin"),
    ("manifest.json", "loom"),
];

/// Find the first existing manifest file in a plugin directory.
fn find_manifest(plugin_dir: &Path) -> Option<(std::path::PathBuf, String)> {
    for (rel_path, format_name) in MANIFEST_CANDIDATES {
        let manifest_path = plugin_dir.join(rel_path);
        if manifest_path.exists() {
            return Some((manifest_path, format_name.to_string()));
        }
    }
    None
}

/// Check if a directory contains a valid plugin manifest (public version).
pub fn find_manifest_in_dir(dir: &Path) -> Option<String> {
    find_manifest(dir).map(|(_, fmt)| fmt)
}

// ── Scanning ──

pub fn scan_plugins_dir(dir: &Path) -> Vec<PluginEntry> {
    let mut plugins = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return plugins,
    };

    for entry in entries.flatten() {
        let plugin_dir = entry.path();
        if !plugin_dir.is_dir() {
            continue;
        }
        // Skip hidden/ignored dirs
        let dir_name = plugin_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if dir_name.starts_with('.') || dir_name == "cache" || dir_name == "data" {
            continue;
        }

        match load_plugin(&plugin_dir) {
            Ok(entry) => plugins.push(entry),
            Err(e) => {
                tracing::debug!(dir = %plugin_dir.display(), error = %e, "Skipping non-plugin directory");
            }
        }
    }
    plugins
}

/// Load a plugin from a directory, detecting the manifest format automatically.
pub fn load_plugin(plugin_dir: &Path) -> Result<PluginEntry> {
    let (manifest_path, format_name) = find_manifest(plugin_dir)
        .ok_or_else(|| anyhow::anyhow!("no recognized plugin manifest found"))?;

    match format_name.as_str() {
        "loom" => load_loom_manifest(&manifest_path, plugin_dir),
        "loom-plugin" | "claude-plugin" | "codex-plugin" => {
            load_universal_manifest(&manifest_path, plugin_dir, &format_name)
        }
        _ => anyhow::bail!("unknown manifest format: {}", format_name),
    }
}

fn load_loom_manifest(manifest_path: &Path, plugin_dir: &Path) -> Result<PluginEntry> {
    let content = std::fs::read_to_string(manifest_path)?;
    let manifest: LoomManifest = serde_json::from_str(&content)?;

    let id = if manifest.id.is_empty() {
        dir_name(plugin_dir)
    } else {
        manifest.id
    };

    Ok(PluginEntry {
        id: id.clone(),
        name: if manifest.name.is_empty() { id } else { manifest.name },
        version: manifest.version,
        description: manifest.description,
        source: "user".to_string(),
        status: "loaded".to_string(),
        trust: if manifest.trust.is_empty() { "restricted".to_string() } else { manifest.trust },
        hidden: manifest.hidden,
        plugin_dir: plugin_dir.to_string_lossy().to_string(),
        manifest_format: "loom".to_string(),
        contributions: detect_contributions(plugin_dir),
        config_schema: manifest.contributes.and_then(|c| c.configuration),
        ui_host_capabilities: manifest.ui.map(|u| u.host_capabilities),
        author: None,
        repository: None,
        category: None,
    })
}

fn load_universal_manifest(
    manifest_path: &Path,
    plugin_dir: &Path,
    format_name: &str,
) -> Result<PluginEntry> {
    let content = std::fs::read_to_string(manifest_path)?;
    let manifest: UniversalManifest = serde_json::from_str(&content)?;

    let id = slugify(&manifest.name);
    let name = manifest
        .interface
        .as_ref()
        .and_then(|i| i.display_name.as_deref())
        .unwrap_or(&manifest.name)
        .to_string();

    let version = if manifest.version.is_empty() { "0.1.0" } else { &manifest.version };

    // Detect contributions from universal layout
    let mut contributions = Vec::new();
    let skills_dir = manifest
        .skills
        .as_ref()
        .and_then(|s| s.path.as_deref())
        .map(|p| plugin_dir.join(p))
        .unwrap_or_else(|| plugin_dir.join("skills"));
    if skills_dir.exists() && has_md_files(&skills_dir) {
        contributions.push("skills".to_string());
    }

    let mcp_dir = manifest
        .mcp_servers
        .as_ref()
        .and_then(|m| m.path.as_deref())
        .map(|p| plugin_dir.join(p))
        .unwrap_or_else(|| plugin_dir.join("mcp-servers"));
    if mcp_dir.exists() {
        contributions.push("mcp-servers".to_string());
    }

    let apps_dir = manifest
        .apps
        .as_ref()
        .and_then(|a| a.path.as_deref())
        .map(|p| plugin_dir.join(p))
        .unwrap_or_else(|| plugin_dir.join("apps"));
    if apps_dir.exists() {
        contributions.push("apps".to_string());
    }

    if manifest.hooks.is_some() || plugin_dir.join("hooks").exists() {
        contributions.push("hooks".to_string());
    }

    // Also check for tools/ directory
    if plugin_dir.join("tools").exists() {
        contributions.push("tools".to_string());
    }

    let category = manifest
        .interface
        .as_ref()
        .and_then(|i| i.category.as_deref())
        .map(String::from);

    Ok(PluginEntry {
        id,
        name,
        version: version.to_string(),
        description: manifest
            .interface
            .as_ref()
            .and_then(|i| i.short_description.as_deref())
            .unwrap_or(&manifest.description)
            .to_string(),
        source: "user".to_string(),
        status: "loaded".to_string(),
        trust: "restricted".to_string(),
        hidden: false,
        plugin_dir: plugin_dir.to_string_lossy().to_string(),
        manifest_format: format_name.to_string(),
        contributions,
        config_schema: None,
        ui_host_capabilities: None,
        author: if manifest.author.is_empty() { None } else { Some(manifest.author) },
        repository: manifest.repository,
        category,
    })
}

fn has_md_files(dir: &Path) -> bool {
    if !dir.exists() {
        return false;
    }
    std::fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .ends_with(".md")
            })
        })
        .unwrap_or(false)
}

fn detect_contributions(plugin_dir: &Path) -> Vec<String> {
    let mut contribs = Vec::new();
    let checks: &[(&str, &str)] = &[
        ("tools", "tools"),
        ("skills", "skills"),
        ("commands", "commands"),
        ("routes", "routes"),
        ("agents", "agents"),
        ("providers", "providers"),
        ("extensions", "extensions"),
        ("index.js", "lifecycle"),
        ("mcp-servers", "mcp-servers"),
        ("apps", "apps"),
        ("hooks", "hooks"),
    ];
    for (subdir, label) in checks {
        if plugin_dir.join(subdir).exists() {
            contribs.push(label.to_string());
        }
    }
    contribs
}

// ── Marketplace scanning ──

/// Scan a marketplace source directory for available plugins.
/// A marketplace source is a directory containing plugin subdirectories.
pub fn scan_marketplace_dir(source_dir: &Path) -> Vec<MarketplacePlugin> {
    let mut plugins = Vec::new();
    let entries = match std::fs::read_dir(source_dir) {
        Ok(e) => e,
        Err(_) => return plugins,
    };

    for entry in entries.flatten() {
        let plugin_dir = entry.path();
        if !plugin_dir.is_dir() {
            continue;
        }
        let dir_name = plugin_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if dir_name.starts_with('.') {
            continue;
        }

        // Try to load the plugin to get its metadata
        match load_plugin(&plugin_dir) {
            Ok(entry) => {
                let ver = entry.version.clone();
                plugins.push(MarketplacePlugin {
                    id: entry.id.clone(),
                    name: entry.name,
                    publisher: entry.author,
                    version: Some(ver.clone()),
                    description: if entry.description.is_empty() { None } else { Some(entry.description) },
                    trust: entry.trust,
                    contributions: entry.contributions,
                    repository: entry.repository,
                    compatibility: MarketplaceCompatibility {
                        min_app_version: None,
                    },
                    distribution: Some(MarketplaceDistribution {
                        kind: "source".to_string(),
                        path: plugin_dir.to_string_lossy().to_string(),
                    }),
                    installed: false,
                    installed_version: None,
                    latest_version: Some(ver),
                    update_available: false,
                    can_install: true,
                    install_action: "install".to_string(),
                    compatible: true,
                })
            }
            Err(_) => continue,
        }
    }
    plugins
}

/// Cross-reference marketplace plugins with installed plugins to set
/// installed/update status.
pub fn cross_reference_marketplace(
    marketplace: &mut [MarketplacePlugin],
    installed: &[PluginEntry],
) {
    let installed_by_id: std::collections::HashMap<&str, &PluginEntry> =
        installed.iter().map(|p| (p.id.as_str(), p)).collect();

    for mp in marketplace.iter_mut() {
        if let Some(inst) = installed_by_id.get(mp.id.as_str()) {
            mp.installed = true;
            mp.installed_version = Some(inst.version.clone());
            let has_update = mp
                .latest_version
                .as_ref()
                .is_some_and(|lv| lv != &inst.version);
            mp.update_available = has_update;
            mp.install_action = if has_update { "update".to_string() } else { "reinstall".to_string() };
        } else {
            mp.installed = false;
            mp.installed_version = None;
            mp.update_available = false;
            mp.install_action = "install".to_string();
            mp.can_install = true;
            mp.compatible = true;
        }
        mp.can_install = mp.compatible;
        if !mp.compatible {
            mp.install_action = "incompatible".to_string();
        }
    }
}

/// Get the default marketplace directory under data_dir.
pub fn marketplace_dir(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("plugins").join("marketplace")
}

// ── Plugin operations ──

pub fn install_plugin(source: &Path, plugins_dir: &Path) -> Result<PluginEntry> {
    if !source.is_dir() {
        anyhow::bail!("plugin source must be a directory");
    }

    // Determine plugin ID
    let plugin_id = match load_plugin(source) {
        Ok(entry) => entry.id,
        Err(_) => dir_name(source),
    };

    let dest_dir = plugins_dir.join(&plugin_id);
    if dest_dir.exists() {
        std::fs::remove_dir_all(&dest_dir)?;
    }
    copy_dir_recursive(source, &dest_dir)?;

    load_plugin(&dest_dir).or_else(|_| {
        Ok(PluginEntry {
            id: plugin_id.clone(),
            name: plugin_id,
            version: "0.1.0".to_string(),
            description: String::new(),
            source: "user".to_string(),
            status: "loaded".to_string(),
            trust: "restricted".to_string(),
            hidden: false,
            plugin_dir: dest_dir.to_string_lossy().to_string(),
            manifest_format: "unknown".to_string(),
            contributions: detect_contributions(&dest_dir),
            config_schema: None,
            ui_host_capabilities: None,
            author: None,
            repository: None,
            category: None,
        })
    })
}

/// Install a marketplace plugin by copying from marketplace dir to plugins dir.
pub fn install_marketplace_plugin(
    mp_plugin: &MarketplacePlugin,
    plugins_dir: &Path,
) -> Result<PluginEntry> {
    let source_path = mp_plugin
        .distribution
        .as_ref()
        .map(|d| Path::new(&d.path).to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("no distribution path"))?;
    install_plugin(&source_path, plugins_dir)
}

pub fn remove_plugin(plugins_dir: &Path, id: &str) -> Result<bool> {
    let dir = plugins_dir.join(id);
    if !dir.exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(true)
}

// ── Helpers ──

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn slugify(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
