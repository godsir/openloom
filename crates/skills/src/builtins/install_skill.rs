use std::path::PathBuf;

use anyhow::Result;
use serde_json::{json, Value};

use crate::{Skill, SkillManifest, SkillPermissions};

pub struct InstallSkillSkill {
    data_dir: PathBuf,
}

impl InstallSkillSkill {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait::async_trait]
impl Skill for InstallSkillSkill {
    fn name(&self) -> &str {
        "install_skill"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "install_skill".into(),
            description: "Install a new skill from a GitHub URL or from provided skill content. The skill is written to the skills directory and becomes available after the next engine restart.".into(),
            triggers: vec!["安装技能".into(), "安装skill".into(), "install skill".into()],
            permissions: SkillPermissions {
                network: Some(vec!["raw.githubusercontent.com".into()]),
                fs_write: Some(vec![]),
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let has_github = params.get("github_url").and_then(|v| v.as_str()).is_some();
        let has_content = params.get("skill_content").and_then(|v| v.as_str()).is_some();

        if has_github {
            return self.install_from_github(&params).await;
        }
        if has_content {
            return self.install_from_content(&params).await;
        }

        Ok(json!({
            "error": "Either 'github_url' or 'skill_content' (+ 'skill_name') is required.",
            "usage": {
                "from_github": { "github_url": "https://github.com/owner/repo/tree/main/path/to/skill", "skill_name": "(optional override)" },
                "from_content": { "skill_content": "markdown content of SKILL.md", "skill_name": "my-skill" }
            }
        }))
    }

    fn context_md(&self) -> &str {
        "install_skill: install a new skill from GitHub or from content."
    }
}

impl InstallSkillSkill {
    async fn install_from_github(&self, params: &Value) -> Result<Value> {
        let github_url = params
            .get("github_url")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let parsed = parse_github_url(github_url);
        if parsed.is_none() {
            return Ok(json!({
                "error": "Could not parse GitHub URL. Expected format: https://github.com/owner/repo/tree/branch/path"
            }));
        }
        let (owner, repo, subpath) = parsed.unwrap();

        // Determine skill name
        let skill_name = params
            .get("skill_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                subpath
                    .rsplit('/')
                    .next()
                    .unwrap_or(&repo)
                    .to_string()
            });

        if let Err(e) = validate_skill_name(&skill_name) {
            return Ok(json!({"error": e}));
        }

        // Fetch SKILL.md from raw.githubusercontent.com
        let raw_url = if subpath.is_empty() {
            format!(
                "https://raw.githubusercontent.com/{}/{}/HEAD/SKILL.md",
                owner, repo
            )
        } else {
            format!(
                "https://raw.githubusercontent.com/{}/{}/HEAD/{}/SKILL.md",
                owner, repo, subpath
            )
        };

        tracing::info!(url = %raw_url, name = %skill_name, "install_skill: fetching from GitHub");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let resp = client
            .get(&raw_url)
            .header("User-Agent", "openLoom-install-skill/0.2")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch {}: {}", raw_url, e))?;

        if !resp.status().is_success() {
            return Ok(json!({
                "error": format!("GitHub returned HTTP {} for {}", resp.status(), raw_url),
                "hint": "Check that the URL is correct and the repository contains a SKILL.md file at that path."
            }));
        }

        let content = resp.text().await?;
        if content.trim().is_empty() {
            return Ok(json!({"error": "Fetched SKILL.md is empty."}));
        }

        self.write_skill(&skill_name, &content)
    }

    async fn install_from_content(&self, params: &Value) -> Result<Value> {
        let skill_name = params
            .get("skill_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let skill_content = params
            .get("skill_content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if skill_name.is_empty() {
            return Ok(json!({"error": "'skill_name' is required when using 'skill_content'."}));
        }
        if skill_content.is_empty() {
            return Ok(json!({"error": "'skill_content' is required and must not be empty."}));
        }
        if let Err(e) = validate_skill_name(skill_name) {
            return Ok(json!({"error": e}));
        }

        self.write_skill(skill_name, skill_content)
    }

    fn write_skill(&self, name: &str, content: &str) -> Result<Value> {
        let skills_dir = self.data_dir.join("skills").join(name);
        std::fs::create_dir_all(&skills_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create skill directory: {}", e))?;

        let skill_path = skills_dir.join("SKILL.md");
        std::fs::write(&skill_path, content)
            .map_err(|e| anyhow::anyhow!("Failed to write SKILL.md: {}", e))?;

        tracing::info!(name = name, path = %skill_path.display(), "install_skill: skill installed");

        Ok(json!({
            "ok": true,
            "name": name,
            "path": skill_path.to_string_lossy(),
            "message": format!("Skill '{}' installed. Restart the engine to load it.", name),
        }))
    }
}

/// Parse a GitHub URL into (owner, repo, subpath).
/// Handles:
///   - https://github.com/owner/repo
///   - https://github.com/owner/repo/tree/branch/path/to/skill
fn parse_github_url(url: &str) -> Option<(String, String, String)> {
    let url = url.trim_end_matches('/');
    // Strip protocol
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("github.com/"))?;

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 2 {
        return None;
    }
    let owner = parts[0].to_string();
    let repo = parts[1].to_string().trim_end_matches(".git").to_string();

    // Check for /tree/branch/path
    if parts.len() >= 4 && parts[2] == "tree" {
        let subpath = parts[4..].join("/");
        Some((owner, repo, subpath))
    } else {
        // Just owner/repo, skill is at root
        Some((owner, repo, String::new()))
    }
}

fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Skill name must not be empty.".into());
    }
    if name.len() > 64 {
        return Err(format!(
            "Skill name '{}' is too long (max 64 characters).",
            name
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "Skill name '{}' contains invalid characters. Use only letters, numbers, dash, and underscore.",
            name
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_url_with_tree_path() {
        let result = parse_github_url(
            "https://github.com/anthropics/skills/tree/main/skills/my-skill",
        );
        assert!(result.is_some());
        let (owner, repo, path) = result.unwrap();
        assert_eq!(owner, "anthropics");
        assert_eq!(repo, "skills");
        assert_eq!(path, "skills/my-skill");
    }

    #[test]
    fn test_parse_github_url_repo_only() {
        let result = parse_github_url("https://github.com/anthropics/skills");
        assert!(result.is_some());
        let (owner, repo, path) = result.unwrap();
        assert_eq!(owner, "anthropics");
        assert_eq!(repo, "skills");
        assert_eq!(path, "");
    }

    #[test]
    fn test_validate_skill_name_good() {
        assert!(validate_skill_name("my-skill").is_ok());
        assert!(validate_skill_name("skill_123").is_ok());
        assert!(validate_skill_name("MySkill").is_ok());
    }

    #[test]
    fn test_validate_skill_name_bad() {
        assert!(validate_skill_name("").is_err());
        assert!(validate_skill_name("skill name").is_err());
        assert!(validate_skill_name("skill/name").is_err());
    }
}
