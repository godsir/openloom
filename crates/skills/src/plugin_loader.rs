use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use crate::external::ExternalSkill;

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
}

pub struct PluginLoader;

impl PluginLoader {
    /// Discover external skills from plugin directories and project-local skills.
    ///
    /// Scans:
    /// - `<data_dir>/plugins/` — recursively finds directories with `.loom-plugin/plugin.json`
    ///   or `.claude-plugin/plugin.json` (supports Claude Code's nested cache structure)
    /// - `<data_dir>/skills/*/SKILL.md` — global standalone skills (plugin_name = "global")
    /// - `<cwd>/.loom/skills/*/` — project-local flat skills (plugin_name = "project")
    pub fn discover(data_dir: &Path, cwd: &Path) -> Vec<ExternalSkill> {
        let mut skills = Vec::new();

        // Recursively scan data_dir/plugins/ for plugin directories
        let plugins_dir = data_dir.join("plugins");
        if plugins_dir.is_dir() {
            Self::scan_plugins_recursive(&plugins_dir, &mut skills, 0);
        }

        // Scan data_dir/skills/*/ (global standalone skills)
        let global_skills_dir = data_dir.join("skills");
        skills.extend(Self::load_flat_skills(&global_skills_dir, "global"));

        // Scan cwd/.loom/skills/*/ (project-local, optional)
        let project_skills_dir = cwd.join(".loom").join("skills");
        skills.extend(Self::load_flat_skills(&project_skills_dir, "project"));

        skills
    }

    fn scan_plugins_recursive(dir: &Path, skills: &mut Vec<ExternalSkill>, depth: usize) {
        if depth > 6 {
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Skip hidden dirs except .loom-plugin/.claude-plugin
            if name_str.starts_with('.') {
                continue;
            }
            // Try loading as a plugin (has .loom-plugin/ or .claude-plugin/)
            match Self::load_plugin(&path) {
                Ok((_manifest, plugin_skills)) => {
                    skills.extend(plugin_skills);
                }
                Err(_) => {
                    // Not a plugin dir — recurse deeper
                    Self::scan_plugins_recursive(&path, skills, depth + 1);
                }
            }
        }
    }

    /// Load a plugin from a directory that contains `.loom-plugin/plugin.json`
    /// or `.claude-plugin/plugin.json`, plus a `skills/` subdirectory.
    fn load_plugin(plugin_dir: &Path) -> Result<(PluginManifest, Vec<ExternalSkill>)> {
        // Try .loom-plugin first, then .claude-plugin
        let manifest_path = {
            let loom = plugin_dir.join(".loom-plugin").join("plugin.json");
            if loom.is_file() {
                loom
            } else {
                let claude = plugin_dir.join(".claude-plugin").join("plugin.json");
                if claude.is_file() {
                    claude
                } else {
                    anyhow::bail!(
                        "no .loom-plugin/plugin.json or .claude-plugin/plugin.json in {}",
                        plugin_dir.display()
                    );
                }
            }
        };

        let manifest_content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PluginManifest = serde_json::from_str(&manifest_content)?;

        let skills_dir = plugin_dir.join("skills");
        let skills = Self::load_flat_skills(&skills_dir, &manifest.name);

        Ok((manifest, skills))
    }

    /// Load flat skills from `<skills_dir>/*/SKILL.md`.
    fn load_flat_skills(skills_dir: &Path, plugin_name: &str) -> Vec<ExternalSkill> {
        let mut skills = Vec::new();

        if !skills_dir.is_dir() {
            return skills;
        }

        let entries = match std::fs::read_dir(skills_dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::debug!("cannot read skills dir {}: {}", skills_dir.display(), e);
                return skills;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            match std::fs::read_to_string(&skill_md) {
                Ok(content) => match ExternalSkill::from_skill_md(&content, plugin_name) {
                    Ok(skill) => {
                        skills.push(skill);
                    }
                    Err(e) => {
                        tracing::debug!("skipping skill {}: {}", skill_md.display(), e);
                    }
                },
                Err(e) => {
                    tracing::debug!("cannot read {}: {}", skill_md.display(), e);
                }
            }
        }

        skills
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a plugin directory structure under `root/plugin_name/`.
    fn setup_plugin(root: &Path, plugin_name: &str, skills: &[(&str, &str)]) {
        let plugin_dir = root.join(plugin_name);
        let meta_dir = plugin_dir.join(".loom-plugin");
        fs::create_dir_all(&meta_dir).unwrap();

        let manifest = serde_json::json!({
            "name": plugin_name,
            "description": format!("Plugin {}", plugin_name),
            "version": "1.0.0"
        });
        fs::write(
            meta_dir.join("plugin.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        for (skill_name, skill_md) in skills {
            let skill_dir = plugin_dir.join("skills").join(skill_name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();
        }
    }

    fn sample_skill_md(name: &str) -> String {
        format!(
            r#"---
name: {name}
description: "Skill {name}"
---

# {name}
Body content for {name}.
"#
        )
    }

    #[test]
    fn test_discover_no_plugins() {
        let data_dir = TempDir::new().unwrap();
        let cwd = TempDir::new().unwrap();
        // Create empty plugins dir
        fs::create_dir_all(data_dir.path().join("plugins")).unwrap();

        let skills = PluginLoader::discover(data_dir.path(), cwd.path());
        assert!(skills.is_empty());
    }

    #[test]
    fn test_discover_plugin_with_skills() {
        let data_dir = TempDir::new().unwrap();
        let cwd = TempDir::new().unwrap();

        let plugins_dir = data_dir.path().join("plugins");
        fs::create_dir_all(&plugins_dir).unwrap();

        setup_plugin(
            &plugins_dir,
            "myplugin",
            &[("greet", &sample_skill_md("greet"))],
        );

        let skills = PluginLoader::discover(data_dir.path(), cwd.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].qualified_name(), "myplugin:greet");
    }

    #[test]
    fn test_discover_project_local_skills() {
        let data_dir = TempDir::new().unwrap();
        let cwd = TempDir::new().unwrap();

        let skill_dir = cwd.path().join(".loom").join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), sample_skill_md("my-skill")).unwrap();

        let skills = PluginLoader::discover(data_dir.path(), cwd.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].qualified_name(), "project:my-skill");
    }

    #[test]
    fn test_discover_claude_plugin_format() {
        let data_dir = TempDir::new().unwrap();
        let cwd = TempDir::new().unwrap();

        let plugins_dir = data_dir.path().join("plugins");
        let plugin_dir = plugins_dir.join("claude-ext");
        let meta_dir = plugin_dir.join(".claude-plugin");
        fs::create_dir_all(&meta_dir).unwrap();

        let manifest = serde_json::json!({
            "name": "claude-ext",
            "description": "A Claude-format plugin"
        });
        fs::write(
            meta_dir.join("plugin.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let skill_dir = plugin_dir.join("skills").join("helper");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), sample_skill_md("helper")).unwrap();

        let skills = PluginLoader::discover(data_dir.path(), cwd.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].qualified_name(), "claude-ext:helper");
    }

    #[test]
    fn test_load_plugin_reads_manifest() {
        let tmp = TempDir::new().unwrap();
        setup_plugin(
            tmp.path(),
            "testplugin",
            &[("alpha", &sample_skill_md("alpha"))],
        );

        let plugin_dir = tmp.path().join("testplugin");
        let (manifest, skills) = PluginLoader::load_plugin(&plugin_dir).unwrap();
        assert_eq!(manifest.name, "testplugin");
        assert_eq!(manifest.description, "Plugin testplugin");
        assert_eq!(manifest.version, Some("1.0.0".to_string()));
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].qualified_name(), "testplugin:alpha");
    }
}
