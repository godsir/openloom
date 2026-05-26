// SPDX-License-Identifier: Apache-2.0
//! Skill/plugin loading for openLoom v2.
//!
//! Parses SKILL.md files (Claude Code / OpenClaw format) and plugin manifests.
//! Supports progressive disclosure: discovery → activation → execution.

use anyhow::Result;
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub compatibility: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,

    // Claude Code extension fields
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub context_mode: Option<String>,
    #[serde(default, alias = "agent")]
    pub fork_agent_type: Option<String>,
    #[serde(default, alias = "disable-model-invocation")]
    pub disable_model_invocation: bool,
    #[serde(default, alias = "user-invocable")]
    pub user_invocable: bool,
    #[serde(default, alias = "argument-hint")]
    pub argument_hint: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default, alias = "when_to_use")]
    pub when_to_use: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub shell: Option<String>,

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

    #[serde(default, flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl CanonicalSkillMetadata {
    /// Check if this skill passes runtime gating: OS, required binaries, required env vars.
    /// Returns `Ok(())` if valid, `Err(reason)` if it should be skipped.
    pub fn validate_runtime(&self) -> Result<(), String> {
        // OS restriction: if set, current OS must be in the list
        if !self.os_restriction.is_empty() {
            let current = if cfg!(windows) { "windows" }
                else if cfg!(target_os = "macos") { "darwin" }
                else { "linux" };
            if !self.os_restriction.iter().any(|o| o == current) {
                return Err(format!("OS restriction: requires {:?}, running {}", self.os_restriction, current));
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
            let found = self.requires_any_bins.iter().any(|b| which::which(b).is_ok());
            if !found {
                return Err(format!("Missing required binary (any of): {:?}", self.requires_any_bins));
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
    Project { cwd: std::path::PathBuf },
    UserGlobal { data_dir: std::path::PathBuf },
    Plugin { plugin_name: String, plugin_dir: std::path::PathBuf },
    Marketplace,
}

// ============================================================================
// Skill Loader
// ============================================================================

pub struct SkillLoader {
    search_paths: Vec<(std::path::PathBuf, String)>,
}

impl SkillLoader {
    pub fn new() -> Self {
        Self { search_paths: Vec::new() }
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
        self.add_path(data_dir.join("skills"), "User Skills");
    }

    pub fn discover(&self) -> Result<Vec<LoadedSkill>> {
        let mut skills = Vec::new();
        for (path, _label) in &self.search_paths {
            if !path.exists() { continue; }
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let skill_dir = entry.path();
                    if !skill_dir.is_dir() { continue; }
                    let skill_md = skill_dir.join("SKILL.md");
                    if skill_md.exists() {
                        match Self::parse_skill_file(&skill_md, &skill_dir) {
                            Ok(skill) => {
                                // Runtime gating
                                if let Err(reason) = skill.manifest.validate_runtime() {
                                    tracing::info!(name=%skill.manifest.name, %reason, "skill skipped");
                                    continue;
                                }
                                skills.push(skill);
                            }
                            Err(e) => tracing::warn!(path = %skill_md.display(), error = %e, "failed to parse skill"),
                        }
                    }
                }
            }
        }
        tracing::info!(count = skills.len(), "skills discovered");
        Ok(skills)
    }

    pub fn parse_skill_file(file_path: &std::path::Path, skill_root: &std::path::Path) -> Result<LoadedSkill> {
        let content = std::fs::read_to_string(file_path)?;
        let (manifest, body) = Self::split_frontmatter(&content, file_path)?;
        Ok(LoadedSkill {
            manifest,
            body,
            source_path: file_path.to_path_buf(),
            skill_root: skill_root.to_path_buf(),
            source: SkillSource::UserGlobal { data_dir: skill_root.to_path_buf() },
        })
    }

    pub fn split_frontmatter(content: &str, file_path: &std::path::Path) -> Result<(CanonicalSkillMetadata, String)> {
        let trimmed = content.trim();
        if !trimmed.starts_with("---") {
            let name = file_path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
            return Ok((CanonicalSkillMetadata { name, description: trimmed.lines().next().unwrap_or("").to_string(), user_invocable: true, ..Default::default() }, trimmed.to_string()));
        }
        let after_first = &trimmed[3..];
        if let Some(end) = after_first.find("\n---") {
            let yaml_str = after_first[..end].trim();
            let body = after_first[end + 4..].trim().to_string();
            let mut manifest: CanonicalSkillMetadata = serde_yaml::from_str(yaml_str)
                .map_err(|e| anyhow::anyhow!("YAML parse error in {}: {}", file_path.display(), e))?;
            if manifest.name.is_empty() {
                manifest.name = file_path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
            }
            Ok((manifest, body))
        } else {
            let name = file_path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
            Ok((CanonicalSkillMetadata { name, description: String::new(), user_invocable: true, ..Default::default() }, trimmed.to_string()))
        }
    }
}

impl Default for SkillLoader {
    fn default() -> Self { Self::new() }
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok().map(std::path::PathBuf::from)
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
}
