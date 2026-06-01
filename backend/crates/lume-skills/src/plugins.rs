//! Plugin system for openLoom v2.
//!
//! Compatible with Claude Code and OpenClaw plugin formats.
//! Scans ~/.claude/plugins/, ~/.openclaw/plugins/, ~/.loom/plugins/.
//! Accepts plugin.toml (our format) and manifest.json / package.json (theirs).
//! Auto-discovers: any directory with a SKILL.md is treated as an implicit plugin.

use anyhow::Result;
use serde::Deserialize;

/// Parsed plugin manifest. Supports both our plugin.toml format and
/// Claude Code / OpenClaw manifest.json formats.
#[derive(Debug, Default)]
pub struct PluginManifest {
    pub name: String,
    #[allow(dead_code)]
    pub version: String,
    pub description: String,
    pub skills: Option<PluginSkillsSection>,
    pub mcp_servers: Option<Vec<PluginMcpServer>>,
}

/// Our native plugin.toml format.
#[derive(Debug, Deserialize)]
struct TomlManifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    skills: Option<PluginSkillsSection>,
    #[serde(default)]
    mcp_servers: Option<Vec<PluginMcpServer>>,
}

/// Claude Code / OpenClaw manifest.json / package.json format.
#[derive(Debug, Deserialize)]
struct JsonManifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    // OpenClaw-style: skills declared as array of name/path objects
    #[serde(default)]
    skills: Option<Vec<JsonSkillEntry>>,
    // Claude Code-style: mcpServers in package.json
    #[serde(default)]
    #[serde(alias = "mcpServers")]
    mcp_servers: Option<std::collections::HashMap<String, JsonMcpEntry>>,
}

#[derive(Debug, Deserialize)]
struct JsonSkillEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonMcpEntry {
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    transport: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginSkillsSection {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginMcpServer {
    pub name: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
}

fn default_transport() -> String {
    "stdio".into()
}

/// A discovered plugin ready for activation.
pub struct DiscoveredPlugin {
    pub manifest: PluginManifest,
    pub path: std::path::PathBuf,
    pub source: String, // "claude", "openclaw", "loom", "auto"
}

/// Discovers and loads plugins from Claude Code / OpenClaw / openLoom directories.
pub struct PluginManager {
    plugins: Vec<DiscoveredPlugin>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Scan all known plugin directories.
    pub fn discover(&mut self, home_dir: &std::path::Path) -> Result<usize> {
        let search: &[(&str, &str)] = &[
            ("~/.claude/plugins", "claude"),
            ("~/.openclaw/plugins", "openclaw"),
            ("~/.loom/plugins", "loom"),
        ];
        for (label, source) in search {
            let path = home_dir.join(label.strip_prefix("~/").unwrap_or(label));
            let before = self.plugins.len();
            self.scan_dir(&path, source);
            let found = self.plugins.len() - before;
            if found > 0 {
                tracing::info!(count=found, dir=%path.display(), source, "plugins discovered");
            }
        }
        Ok(self.plugins.len())
    }

    /// Recursively scan a directory up to max_depth levels for plugin manifests / SKILL.md files.
    fn scan_dir(&mut self, dir: &std::path::Path, source: &str) {
        self.scan_dir_depth(dir, source, 0);
    }

    fn scan_dir_depth(&mut self, dir: &std::path::Path, source: &str, depth: u32) {
        if !dir.exists() || depth > 4 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let plugin_dir = entry.path();
            let dir_name = entry.file_name().to_string_lossy().to_string();

            // Skip meta dirs
            let name = dir_name.to_lowercase();
            if name == "node_modules" || name == ".git" || name.starts_with('.') {
                continue;
            }

            // Try loading a manifest at this level
            if let Some(mut manifest) = self
                .try_load_toml(&plugin_dir)
                .or_else(|| self.try_load_json(&plugin_dir))
            {
                if manifest.name.is_empty() {
                    manifest.name = dir_name.clone();
                }
                self.plugins.push(DiscoveredPlugin {
                    manifest,
                    path: plugin_dir,
                    source: source.to_string(),
                });
                continue; // found a plugin, don't recurse deeper into it
            }

            // Auto-discover: directory with SKILL.md but no manifest
            if self.has_skill_md(&plugin_dir) {
                self.plugins.push(DiscoveredPlugin {
                    manifest: PluginManifest {
                        name: dir_name,
                        skills: Some(PluginSkillsSection {
                            paths: vec![".".into()],
                        }),
                        ..Default::default()
                    },
                    path: plugin_dir,
                    source: format!("{}-auto", source),
                });
                continue;
            }

            // Recurse deeper for marketplace-style nesting
            // e.g. ~/.claude/plugins/<marketplace>/plugins/<name>/
            self.scan_dir_depth(&plugin_dir, source, depth + 1);
        }
    }

    fn try_load_toml(&self, dir: &std::path::Path) -> Option<PluginManifest> {
        let path = dir.join("plugin.toml");
        let content = std::fs::read_to_string(&path).ok()?;
        let tm: TomlManifest = toml::from_str(&content).ok()?;
        Some(PluginManifest {
            name: tm.name,
            version: tm.version,
            description: tm.description,
            skills: tm.skills,
            mcp_servers: tm.mcp_servers,
        })
    }

    fn try_load_json(&self, dir: &std::path::Path) -> Option<PluginManifest> {
        let path = dir
            .join("manifest.json")
            .exists()
            .then(|| dir.join("manifest.json"))
            .or_else(|| {
                dir.join("package.json")
                    .exists()
                    .then(|| dir.join("package.json"))
            })?;
        let content = std::fs::read_to_string(&path).ok()?;
        let jm: JsonManifest = serde_json::from_str(&content).ok()?;

        // Convert OpenClaw-style skills to our format
        let skills = jm.skills.map(|entries| {
            let paths: Vec<String> = entries
                .iter()
                .map(|e| e.path.clone().unwrap_or_else(|| e.name.clone()))
                .collect();
            PluginSkillsSection { paths }
        });

        // Convert Claude Code-style mcpServers to our format
        let mcp_servers = jm.mcp_servers.map(|map| {
            map.into_iter()
                .map(|(name, entry)| PluginMcpServer {
                    name,
                    transport: entry.transport.unwrap_or_else(|| {
                        if entry.url.is_some() {
                            "http".into()
                        } else {
                            "stdio".into()
                        }
                    }),
                    command: entry.command,
                    args: entry.args,
                    url: entry.url,
                    headers: Default::default(),
                })
                .collect()
        });

        Some(PluginManifest {
            name: jm.name,
            version: jm.version,
            description: jm.description,
            skills,
            mcp_servers,
        })
    }

    fn has_skill_md(&self, dir: &std::path::Path) -> bool {
        // Check if directory itself has SKILL.md
        if dir.join("SKILL.md").exists() {
            return true;
        }
        // Check one level of subdirectories
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    && entry.path().join("SKILL.md").exists()
                {
                    return true;
                }
            }
        }
        false
    }

    /// List discovered plugins with source and description.
    pub fn list(&self) -> Vec<(&str, &str, &str)> {
        self.plugins
            .iter()
            .map(|p| {
                (
                    p.manifest.name.as_str(),
                    p.manifest.description.as_str(),
                    p.source.as_str(),
                )
            })
            .collect()
    }

    /// Collect all skill paths from plugins.
    pub fn skill_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        for p in &self.plugins {
            if let Some(ref skills) = p.manifest.skills {
                for sp in &skills.paths {
                    paths.push(p.path.join(sp));
                }
            }
        }
        paths
    }

    /// Collect all MCP server configs from plugins.
    pub fn mcp_configs(&self) -> Vec<PluginMcpServer> {
        let mut configs = Vec::new();
        for p in &self.plugins {
            if let Some(ref servers) = p.manifest.mcp_servers {
                configs.extend(servers.clone());
            }
        }
        configs
    }
}
