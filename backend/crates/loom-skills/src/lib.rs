// SPDX-License-Identifier: Apache-2.0
//! Skill loading for openLoom v2.
//!
//! Parses SKILL.md files (Claude Code / OpenClaw format).

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Skill permission configuration — parsed from SKILL.md YAML frontmatter.
/// Mirrors the fields in `loom_types::SkillPermissions` but all optional,
/// so a skill can declare only the capabilities it needs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillPermissionConfig {
    #[serde(default)]
    pub shell: Option<bool>,
    #[serde(default)]
    pub fs_write: Option<Vec<String>>,
    #[serde(default)]
    pub fs_read: Option<Vec<String>>,
    #[serde(default)]
    pub network: Option<Vec<String>>,
    #[serde(default)]
    pub subprocess: Option<bool>,
}

/// Complete parsed metadata from a SKILL.md YAML frontmatter.
/// Union of Claude Code, OpenClaw, and agentskills.io fields.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CanonicalSkillMetadata {
    // Required
    pub name: String,
    pub description: String,

    // Common optional
    #[serde(default)]
    pub version: Option<String>,

    // Claude Code / OpenClaw extension fields (parsed for forward compatibility;
    // most are not yet consumed — see #[allow(dead_code)] annotations below)
    #[serde(default)]
    // Per-skill tool allowlist — union of active skills' allowed_tools gates the
    // tool definitions sent to the model at the agent-loop level.
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    // TODO: Planned — skill-specific model override
    pub model: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    // TODO: Planned — effort level for reasoning (Claude Code parity)
    pub effort: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    // TODO: Planned — context window mode (e.g. "full" vs "compact")
    pub context_mode: Option<String>,
    #[serde(default, alias = "agent")]
    #[allow(dead_code)]
    // TODO: Planned — agent subprocess type for skill execution
    pub fork_agent_type: Option<String>,
    #[serde(default, alias = "disable-model-invocation")]
    #[allow(dead_code)]
    // TODO: Planned — skill that uses tools without model invocation
    pub disable_model_invocation: bool,
    #[serde(default, alias = "user-invocable")]
    pub user_invocable: bool,
    #[serde(default, alias = "argument-hint")]
    #[allow(dead_code)]
    // TODO: Planned — help text displayed when prompting for arguments
    pub argument_hint: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    // TODO: Planned — predefined argument list for skill invocation
    pub arguments: Vec<String>,
    #[serde(default, alias = "when_to_use")]
    #[allow(dead_code)]
    // TODO: Planned — guidance text for when this skill should be selected
    pub when_to_use: Option<String>,
    // NOTE: `paths` and `shell` removed — OpenClaw-specific fields not needed by this codebase.

    // OpenClaw runtime gating
    #[serde(default)]
    pub requires_env: Vec<String>,
    #[serde(default)]
    pub requires_bins: Vec<String>,
    #[serde(default)]
    pub requires_any_bins: Vec<String>,
    #[serde(default)]
    pub requires_config: Vec<String>,
    #[serde(default)]
    pub os_restriction: Vec<String>,
    #[serde(default)]
    pub always_active: bool,

    #[serde(default)]
    pub permissions: Option<SkillPermissionConfig>,

    #[serde(default, flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl CanonicalSkillMetadata {
    /// Build a one-line summary for skill context injection.
    /// Includes version when present: `"- name (v1.2.3): description"`.
    pub fn summary_line(&self) -> String {
        let name = self.name.replace('\n', " ").replace('\r', "");
        match &self.version {
            Some(v) => format!("- {} (v{}): {}", name, v, self.description),
            None => format!("- {}: {}", name, self.description),
        }
    }

    /// Check if this skill passes runtime gating: OS, required binaries, required env vars.
    /// Returns `Ok(())` if valid, `Err(reason)` if it should be skipped.
    pub fn validate_runtime(&self) -> Result<(), String> {
        // OS restriction: if set, current OS must be in the list
        if !self.os_restriction.is_empty() {
            let current = if cfg!(windows) {
                "windows"
            } else if cfg!(target_os = "macos") {
                "darwin"
            } else {
                "linux"
            };
            if !self.os_restriction.iter().any(|o| o == current) {
                return Err(format!(
                    "OS restriction: requires {:?}, running {}",
                    self.os_restriction, current
                ));
            }
        }

        // Required binaries: ALL must be in PATH
        for bin in &self.requires_bins {
            if which::which(bin).is_err() {
                return Err(format!("Missing required binary: '{}'", bin));
            }
        }

        // Required any bins: at least ONE must be in PATH
        if !self.requires_any_bins.is_empty() {
            let found = self
                .requires_any_bins
                .iter()
                .any(|b| which::which(b).is_ok());
            if !found {
                return Err(format!(
                    "Missing required binary (any of): {:?}",
                    self.requires_any_bins
                ));
            }
        }

        // Required env vars: ALL must be set
        for env in &self.requires_env {
            if std::env::var(env).is_err() {
                return Err(format!("Missing required env var: '{}'", env));
            }
        }

        // Required config paths: ALL must exist
        for cfg_path in &self.requires_config {
            if !std::path::Path::new(cfg_path).exists() {
                return Err(format!("Missing required config: '{}'", cfg_path));
            }
        }

        Ok(())
    }
}

/// A loaded skill ready for injection into agent context.
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub manifest: CanonicalSkillMetadata,
    pub body: String,
    pub source_path: std::path::PathBuf,
    pub skill_root: std::path::PathBuf,
    pub source: SkillSource,
}

#[derive(Debug, Clone)]
pub enum SkillSource {
    Project {
        cwd: std::path::PathBuf,
    },
    UserGlobal {
        data_dir: std::path::PathBuf,
    },
    Plugin {
        plugin_name: String,
        plugin_dir: std::path::PathBuf,
    },
    Marketplace,
}

// ============================================================================
// SkillState — unified snapshot of all loaded skills
// ============================================================================

/// Unified in-memory snapshot of all loaded skills.
///
/// Replaces three separate maps (context string, bodies, permissions) with a single
/// struct behind one `Arc<RwLock<>>`. Includes lightweight summaries for
/// `skills.list` RPC responses.
#[derive(Debug, Clone, Default)]
pub struct SkillState {
    /// Pre-formatted context string injected into the system prompt
    /// (one `"- name: description"` line per skill).
    pub context: String,
    /// Skill name → full SKILL.md body.
    pub bodies: std::collections::HashMap<String, String>,
    /// Skill name → parsed permission config.
    pub permissions: std::collections::HashMap<String, SkillPermissionConfig>,
    /// Skill name → allowed tool names (from `allowed_tools` frontmatter).
    /// Only populated for skills that declare an allowlist.
    pub allowed_tools: std::collections::HashMap<String, Vec<String>>,
    /// Lightweight metadata for all loaded skills (for RPC list responses).
    pub summaries: Vec<SkillSummary>,
}

/// Lightweight skill metadata for `skills.list` RPC responses.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub source_path: String,
    pub version: Option<String>,
    pub user_invocable: bool,
    pub always_active: bool,
}

impl SkillState {
    /// Build a unified `SkillState` from a slice of loaded skills.
    ///
    /// Centralises the `name.replace('\n', " ").replace('\r', "")` normalisation
    /// that was previously duplicated at every call site.
    pub fn from_skills(skills: &[LoadedSkill]) -> Self {
        let mut context = String::new();
        let mut bodies = std::collections::HashMap::new();
        let mut permissions = std::collections::HashMap::new();
        let mut summaries = Vec::with_capacity(skills.len());

        for s in skills {
            let name = s.manifest.name.replace('\n', " ").replace('\r', "");
            let desc = s.manifest.description.clone();
            let path = s.source_path.display().to_string();
            let ver = s.manifest.version.clone();

            // Context line
            if !context.is_empty() {
                context.push('\n');
            }
            context.push_str(&format!("- {}: {}", name, desc));

            // Body
            bodies.insert(name.clone(), s.body.clone());

            // Permissions
            if let Some(ref p) = s.manifest.permissions {
                permissions.insert(name.clone(), p.clone());
            }

            // Summary
            summaries.push(SkillSummary {
                name,
                description: desc,
                source_path: path,
                version: ver,
                user_invocable: s.manifest.user_invocable,
                always_active: s.manifest.always_active,
            });
        }

        // Allowed tools per skill (only for skills that declare an allowlist)
        let mut allowed_tools = std::collections::HashMap::new();
        for s in skills {
            if !s.manifest.allowed_tools.is_empty() {
                allowed_tools.insert(
                    s.manifest.name.replace('\n', " ").replace('\r', ""),
                    s.manifest.allowed_tools.clone(),
                );
            }
        }

        Self {
            context,
            bodies,
            permissions,
            allowed_tools,
            summaries,
        }
    }
}

// ============================================================================
// Skill Loader
// ============================================================================

pub struct SkillLoader {
    search_paths: Vec<(std::path::PathBuf, String)>,
}

impl SkillLoader {
    pub fn new() -> Self {
        Self {
            search_paths: Vec::new(),
        }
    }

    pub fn add_path(&mut self, path: std::path::PathBuf, label: &str) {
        self.search_paths.push((path, label.to_string()));
    }

    pub fn add_standard_paths(&mut self, data_dir: &std::path::Path) {
        if let Ok(cwd) = std::env::current_dir() {
            self.add_path(cwd.join(".loom/skills"), "Project (openLoom)");
            self.add_path(cwd.join(".claude/skills"), "Project (Claude Code)");
        }
        if let Some(home) = dirs_home() {
            self.add_path(home.join(".loom/skills"), "User (openLoom)");
            self.add_path(home.join(".claude/skills"), "User (Claude Code)");
            self.add_path(home.join(".openclaw/skills"), "User (OpenClaw)");
            self.add_path(home.join(".codex/skills"), "User (Codex)");
            self.add_path(home.join(".agents/skills"), "User (Agents)");
        }
        self.add_path(data_dir.join("plugins"), "Plugins");
    }

    pub fn discover(&self) -> Result<Vec<LoadedSkill>> {
        let mut skills = Vec::new();
        for (path, _label) in &self.search_paths {
            if !path.exists() {
                continue;
            }
            // Walk up to 2 levels deep to find SKILL.md files
            // (handles monorepo clones like skills-official where skills are in subdirectories)
            Self::scan_dir(path, _label, 0, 2, &mut skills);
        }
        tracing::info!(count = skills.len(), "skills discovered");
        Ok(skills)
    }

    fn scan_dir(
        dir: &std::path::Path,
        label: &str,
        depth: usize,
        max_depth: usize,
        skills: &mut Vec<LoadedSkill>,
    ) {
        if depth > max_depth {
            return;
        }
        let skill_md = dir.join("SKILL.md");
        if skill_md.exists() {
            match Self::parse_skill_file(&skill_md, dir) {
                Ok(mut skill) => {
                    skill.source = match label {
                        s if s.contains("Project") => SkillSource::Project {
                            cwd: std::env::current_dir().unwrap_or_default(),
                        },
                        s if s.contains("Plugin") => SkillSource::Plugin {
                            plugin_name: String::new(),
                            plugin_dir: dir.to_path_buf(),
                        },
                        _ => SkillSource::UserGlobal {
                            data_dir: dir.to_path_buf(),
                        },
                    };
                    if let Err(reason) = skill.manifest.validate_runtime() {
                        tracing::info!(name=%skill.manifest.name, %reason, "skill skipped");
                        return;
                    }
                    skills.push(skill);
                    return; // Found SKILL.md here, don't recurse into this dir
                }
                Err(e) => {
                    tracing::warn!(path = %skill_md.display(), error = %e, "failed to parse skill")
                }
            }
        }
        // Recurse into subdirectories
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let sub = entry.path();
                if sub.is_dir() {
                    Self::scan_dir(&sub, label, depth + 1, max_depth, skills);
                }
            }
        }
    }

    pub fn parse_skill_file(
        file_path: &std::path::Path,
        skill_root: &std::path::Path,
    ) -> Result<LoadedSkill> {
        let content = std::fs::read_to_string(file_path)?;
        let (manifest, body) = Self::split_frontmatter(&content, file_path)?;
        Ok(LoadedSkill {
            manifest,
            body,
            source_path: file_path.to_path_buf(),
            skill_root: skill_root.to_path_buf(),
            source: SkillSource::UserGlobal {
                data_dir: skill_root.to_path_buf(),
            },
        })
    }

    pub fn split_frontmatter(
        content: &str,
        file_path: &std::path::Path,
    ) -> Result<(CanonicalSkillMetadata, String)> {
        let trimmed = content.trim();
        if !trimmed.starts_with("---") {
            let name = file_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            return Ok((
                CanonicalSkillMetadata {
                    name,
                    description: trimmed.lines().next().unwrap_or("").to_string(),
                    user_invocable: true,
                    ..Default::default()
                },
                trimmed.to_string(),
            ));
        }
        let after_first = &trimmed[3..];
        if let Some(end) = after_first.find("\n---") {
            let yaml_str = after_first[..end].trim();
            let body = after_first[end + 4..].trim().to_string();
            let mut manifest: CanonicalSkillMetadata =
                serde_yaml::from_str(yaml_str).map_err(|e| {
                    anyhow::anyhow!("YAML parse error in {}: {}", file_path.display(), e)
                })?;
            if manifest.name.is_empty() {
                manifest.name = file_path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
            }
            Ok((manifest, body))
        } else {
            let name = file_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            Ok((
                CanonicalSkillMetadata {
                    name,
                    description: String::new(),
                    user_invocable: true,
                    ..Default::default()
                },
                trimmed.to_string(),
            ))
        }
    }
}

impl Default for SkillLoader {
    fn default() -> Self {
        Self::new()
    }
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_with_frontmatter() {
        let content = "---\nname: test-skill\ndescription: A test skill\n---\n\n# Instructions\nDo the thing.";
        let tmp = std::path::PathBuf::from("/tmp/test-skill/SKILL.md");
        let (manifest, body) = SkillLoader::split_frontmatter(content, &tmp).unwrap();
        assert_eq!(manifest.name, "test-skill");
        assert_eq!(manifest.description, "A test skill");
        assert!(body.contains("# Instructions"));
    }

    #[test]
    fn test_parse_skill_without_frontmatter() {
        let content = "# Just markdown\nNo frontmatter here.";
        let tmp = std::path::PathBuf::from("/tmp/myskill/SKILL.md");
        let (manifest, _body) = SkillLoader::split_frontmatter(content, &tmp).unwrap();
        assert_eq!(manifest.name, "myskill");
    }

    #[test]
    fn test_parse_openclaw_metadata() {
        let content = "---\nname: claws\ndescription: test\nrequires_bins:\n  - jq\n  - curl\nos_restriction:\n  - darwin\n  - linux\nalways_active: true\n---\n\nbody";
        let tmp = std::path::PathBuf::from("/tmp/claws/SKILL.md");
        let (manifest, _body) = SkillLoader::split_frontmatter(content, &tmp).unwrap();
        assert_eq!(manifest.requires_bins.len(), 2);
        assert_eq!(manifest.os_restriction.len(), 2);
        assert!(manifest.always_active);
    }

    #[test]
    fn test_skill_state_from_skills() {
        let skills = vec![
            LoadedSkill {
                manifest: CanonicalSkillMetadata {
                    name: "alpha".into(),
                    description: "Alpha skill".into(),
                    version: Some("1.0".into()),
                    user_invocable: true,
                    always_active: false,
                    permissions: Some(SkillPermissionConfig {
                        shell: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                body: "Alpha body".into(),
                source_path: std::path::PathBuf::from("/tmp/alpha/SKILL.md"),
                skill_root: std::path::PathBuf::from("/tmp/alpha"),
                source: SkillSource::UserGlobal {
                    data_dir: std::path::PathBuf::from("/tmp/alpha"),
                },
            },
            LoadedSkill {
                manifest: CanonicalSkillMetadata {
                    name: "beta".into(),
                    description: "Beta skill".into(),
                    version: None,
                    user_invocable: false,
                    always_active: true,
                    permissions: None,
                    ..Default::default()
                },
                body: "Beta body".into(),
                source_path: std::path::PathBuf::from("/tmp/beta/SKILL.md"),
                skill_root: std::path::PathBuf::from("/tmp/beta"),
                source: SkillSource::UserGlobal {
                    data_dir: std::path::PathBuf::from("/tmp/beta"),
                },
            },
        ];

        let state = SkillState::from_skills(&skills);

        // Context contains both skill names
        assert!(state.context.contains("alpha"));
        assert!(state.context.contains("beta"));

        // Bodies map has 2 entries
        assert_eq!(state.bodies.len(), 2);
        assert_eq!(
            state.bodies.get("alpha").map(|s| s.as_str()),
            Some("Alpha body")
        );
        assert_eq!(
            state.bodies.get("beta").map(|s| s.as_str()),
            Some("Beta body")
        );

        // Permissions map has only 1 entry (the one with permissions)
        assert_eq!(state.permissions.len(), 1);
        assert!(state.permissions.contains_key("alpha"));
        assert!(state.permissions.get("alpha").unwrap().shell.unwrap());

        // Summaries has 2 entries with correct fields
        assert_eq!(state.summaries.len(), 2);
        let alpha_summary = state.summaries.iter().find(|s| s.name == "alpha").unwrap();
        assert_eq!(alpha_summary.description, "Alpha skill");
        assert_eq!(alpha_summary.version.as_deref(), Some("1.0"));
        assert!(alpha_summary.user_invocable);
        assert!(!alpha_summary.always_active);

        let beta_summary = state.summaries.iter().find(|s| s.name == "beta").unwrap();
        assert_eq!(beta_summary.description, "Beta skill");
        assert_eq!(beta_summary.version, None);
        assert!(!beta_summary.user_invocable);
        assert!(beta_summary.always_active);
    }

    #[test]
    fn test_skill_state_empty() {
        let state = SkillState::from_skills(&[]);

        assert!(state.context.is_empty());
        assert!(state.bodies.is_empty());
        assert!(state.permissions.is_empty());
        assert!(state.summaries.is_empty());
    }

    #[test]
    fn test_skill_summary_line_with_version() {
        let meta = CanonicalSkillMetadata {
            name: "test".into(),
            description: "desc".into(),
            version: Some("1.0".into()),
            ..Default::default()
        };
        let line = meta.summary_line();
        assert!(line.contains("(v1.0)"));
    }

    #[test]
    fn test_skill_summary_line_without_version() {
        let meta = CanonicalSkillMetadata {
            name: "test".into(),
            description: "desc".into(),
            version: None,
            ..Default::default()
        };
        let line = meta.summary_line();
        assert!(!line.contains("(v"));
    }
}
