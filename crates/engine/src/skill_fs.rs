// Skill filesystem management for openLoom.
//
// Scans .loom/skills/ (and external paths) for SKILL.md files,
// parses their YAML frontmatter, and manages install/delete operations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Parsed SKILL.md metadata (YAML frontmatter subset).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillMetadata {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    #[serde(alias = "defaultEnabled")]
    pub default_enabled: bool,
    #[serde(default)]
    #[serde(alias = "disableModelInvocation")]
    pub disable_model_invocation: bool,
}

fn default_true() -> bool {
    true
}

/// A user skill discovered from the filesystem.
#[derive(Debug, Clone, Serialize)]
pub struct UserSkill {
    pub name: String,
    pub description: String,
    pub file_path: String,
    pub base_dir: String,
    pub source: String, // "user", "learned", "external"
    pub default_enabled: bool,
    pub disable_model_invocation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learned_agent_id: Option<String>,
}

/// Discovered external skill path info (from known tool dirs).
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredExternalPath {
    pub dir_path: String,
    pub label: String,
    pub exists: bool,
}

/// Parse YAML frontmatter from SKILL.md content.
/// Frontmatter is delimited by `---` lines at the start of the file.
pub fn parse_skill_frontmatter(content: &str) -> SkillMetadata {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return SkillMetadata::default();
    }
    // Find the closing ---
    let after_first = &trimmed[3..];
    let end = after_first.find("\n---");
    let yaml_str = match end {
        Some(pos) => &after_first[..pos],
        None => after_first, // No closing ---, take rest
    };

    serde_yaml::from_str::<SkillMetadata>(yaml_str).unwrap_or_default()
}

/// Scan a directory for skill subdirectories (each containing a SKILL.md).
/// Returns discovered skills.
pub fn scan_skills_dir(dir: &Path, source: &str) -> Vec<UserSkill> {
    let mut skills = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return skills,
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let skill_md = entry_path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let content = match std::fs::read_to_string(&skill_md) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let meta = parse_skill_frontmatter(&content);
        let name = if meta.name.is_empty() {
            entry_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            meta.name.clone()
        };

        skills.push(UserSkill {
            name,
            description: meta.description,
            file_path: skill_md.to_string_lossy().to_string(),
            base_dir: entry_path.to_string_lossy().to_string(),
            source: source.to_string(),
            default_enabled: meta.default_enabled,
            disable_model_invocation: meta.disable_model_invocation,
            external_label: None,
            external_path: None,
            learned_agent_id: None,
        });
    }
    skills
}

/// Scan a learned-skills directory (agent-specific).
pub fn scan_learned_skills_dir(dir: &Path, agent_id: &str) -> Vec<UserSkill> {
    let mut skills = scan_skills_dir(dir, "learned");
    for s in &mut skills {
        s.learned_agent_id = Some(agent_id.to_string());
    }
    skills
}

/// Scan multiple external paths for skills.
pub fn scan_external_paths(paths: &[(String, String)]) -> Vec<UserSkill> {
    let mut skills = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (dir_path, label) in paths {
        let dir = Path::new(dir_path);
        if !dir.exists() {
            continue;
        }
        for skill in scan_skills_dir(dir, "external") {
            if seen.insert(skill.name.clone()) {
                skills.push(UserSkill {
                    external_label: Some(label.clone()),
                    external_path: Some(dir_path.clone()),
                    ..skill
                });
            }
        }
    }
    skills
}

/// Discover external skill paths from known tool directories in the home dir.
pub fn discover_external_paths(home_dir: &Path) -> Vec<DiscoveredExternalPath> {
    let patterns: &[(&str, &str)] = &[
        (".claude/skills", "Claude Code"),
        (".codex/skills", "Codex"),
        (".openclaw/skills", "OpenClaw"),
        (".pi/agent/skills", "Pi"),
        (".agents/skills", "Agents"),
    ];

    patterns
        .iter()
        .map(|(suffix, label)| {
            let dir_path = home_dir.join(suffix);
            DiscoveredExternalPath {
                dir_path: dir_path.to_string_lossy().to_string(),
                label: label.to_string(),
                exists: dir_path.exists(),
            }
        })
        .collect()
}

/// Install a skill: copy from source path into the skills directory.
/// The source can be a SKILL.md file (we copy its parent directory) or a directory.
pub fn install_skill(source: &Path, skills_dir: &Path) -> Result<UserSkill> {
    let (skill_dir, skill_md_path) = if source.is_dir() {
        let md = source.join("SKILL.md");
        (source.to_path_buf(), md)
    } else if source
        .file_name()
        .map(|n| n == "SKILL.md")
        .unwrap_or(false)
    {
        let dir = source
            .parent()
            .context("skill file has no parent directory")?
            .to_path_buf();
        (dir.clone(), source.to_path_buf())
    } else {
        anyhow::bail!("source must be a SKILL.md file or a directory containing one");
    };

    if !skill_md_path.exists() {
        anyhow::bail!("SKILL.md not found at {}", skill_md_path.display());
    }

    let content = std::fs::read_to_string(&skill_md_path)?;
    let meta = parse_skill_frontmatter(&content);
    let name = if meta.name.is_empty() {
        skill_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        meta.name.clone()
    };

    let dest_dir = skills_dir.join(&name);
    if dest_dir.exists() {
        // Remove existing to allow overwrite
        std::fs::remove_dir_all(&dest_dir)?;
    }
    copy_dir_recursive(&skill_dir, &dest_dir)?;

    Ok(UserSkill {
        name,
        description: meta.description,
        file_path: dest_dir
            .join("SKILL.md")
            .to_string_lossy()
            .to_string(),
        base_dir: dest_dir.to_string_lossy().to_string(),
        source: "user".to_string(),
        default_enabled: meta.default_enabled,
        disable_model_invocation: meta.disable_model_invocation,
        external_label: None,
        external_path: None,
        learned_agent_id: None,
    })
}

/// Delete a skill by name from the skills directory.
pub fn delete_skill(skills_dir: &Path, name: &str) -> Result<bool> {
    let dir = skills_dir.join(name);
    if !dir.exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(true)
}

/// Recursively copy a directory.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\nname: my-skill\ndescription: Does things\n---\n\n# My Skill\n";
        let meta = parse_skill_frontmatter(content);
        assert_eq!(meta.name, "my-skill");
        assert_eq!(meta.description, "Does things");
    }

    #[test]
    fn test_parse_frontmatter_default_enabled_false() {
        let content = "---\nname: opt-in\ndefaultEnabled: false\n---\n\nBody\n";
        let meta = parse_skill_frontmatter(content);
        assert_eq!(meta.name, "opt-in");
        assert!(!meta.default_enabled);
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let content = "# Just a heading\n\nNo frontmatter here.\n";
        let meta = parse_skill_frontmatter(content);
        assert!(meta.name.is_empty());
    }
}
