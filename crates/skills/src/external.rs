use std::sync::OnceLock;

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Skill, SkillManifest, SkillPermissions};

#[derive(Debug, Clone, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    #[allow(dead_code)]
    license: Option<String>,
}

pub struct ExternalSkill {
    frontmatter: SkillFrontmatter,
    body: String,
    qualified_name: String,
    manifest: OnceLock<SkillManifest>,
}

impl ExternalSkill {
    /// Parse a SKILL.md file: YAML frontmatter between `---` markers, body after second `---`.
    pub fn from_skill_md(content: &str, plugin_name: &str) -> Result<Self> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            bail!("SKILL.md must start with YAML frontmatter delimited by ---");
        }

        // Skip the opening ---
        let after_open = &trimmed[3..];
        let close_pos = after_open
            .find("\n---")
            .ok_or_else(|| anyhow::anyhow!("missing closing --- for YAML frontmatter"))?;

        let yaml_str = &after_open[..close_pos];
        let frontmatter: SkillFrontmatter = serde_yaml::from_str(yaml_str)?;

        // Body starts after the closing --- line
        let rest = &after_open[close_pos + 4..]; // skip "\n---"
        let body = rest.trim_start_matches(['\r', '\n']).to_string();

        let qualified_name = format!("{}:{}", plugin_name, frontmatter.name);

        Ok(Self {
            frontmatter,
            body,
            qualified_name,
            manifest: OnceLock::new(),
        })
    }

    pub fn qualified_name(&self) -> &str {
        &self.qualified_name
    }
}

#[async_trait::async_trait]
impl Skill for ExternalSkill {
    fn name(&self) -> &str {
        &self.qualified_name
    }

    fn manifest(&self) -> &SkillManifest {
        self.manifest.get_or_init(|| SkillManifest {
            name: self.qualified_name.clone(),
            description: self.frontmatter.description.clone(),
            triggers: Vec::new(),
            permissions: SkillPermissions::default(),
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, _params: Value) -> Result<Value> {
        Ok(json!({
            "skill": self.qualified_name,
            "context": self.body,
        }))
    }

    fn context_md(&self) -> &str {
        &self.body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_md() {
        let content = r#"---
name: brainstorming
description: "Explore user intent before implementation"
---

# Brainstorming
Body markdown content here...
"#;
        let skill = ExternalSkill::from_skill_md(content, "plugin").unwrap();
        assert_eq!(skill.qualified_name(), "plugin:brainstorming");
        assert!(skill.context_md().contains("# Brainstorming"));
        assert!(skill.context_md().contains("Body markdown content here..."));
        assert_eq!(skill.name(), "plugin:brainstorming");
        assert_eq!(
            skill.manifest().description,
            "Explore user intent before implementation"
        );
    }

    #[test]
    fn test_parse_skill_md_no_frontmatter_fails() {
        let content = "# Just a markdown file\nNo frontmatter here.";
        let result = ExternalSkill::from_skill_md(content, "plugin");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_skill_md_with_license() {
        let content = r#"---
name: licensed-skill
description: "A skill with a license"
license: MIT
---

Licensed body content.
"#;
        let skill = ExternalSkill::from_skill_md(content, "vendor").unwrap();
        assert_eq!(skill.qualified_name(), "vendor:licensed-skill");
        assert_eq!(skill.frontmatter.license, Some("MIT".to_string()));
        assert!(skill.context_md().contains("Licensed body content."));
    }
}
