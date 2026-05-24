use std::collections::HashSet;
use std::sync::RwLock;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod builtins;
pub mod cron_store;
pub mod external;
pub mod loom_context;
pub mod plugin_loader;
pub mod settings_registry;

/// Skill trait — Phase 1: Rust native implementation; Phase 2: WASM compilation
#[async_trait::async_trait]
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn manifest(&self) -> &SkillManifest;
    async fn invoke(&self, params: Value) -> Result<Value>;
    fn context_md(&self) -> &str;
}

/// Permission model — now defined in openloom-models for shared use with sandbox crate
pub use openloom_models::SkillPermissions;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    #[serde(default)]
    pub permissions: SkillPermissions,
    pub min_engine_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
}

pub struct SkillRegistry {
    skills: Vec<Box<dyn Skill>>,
    disabled: RwLock<HashSet<String>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { skills: Vec::new(), disabled: RwLock::new(HashSet::new()) }
    }

    pub fn register(&mut self, skill: Box<dyn Skill>) {
        self.skills.push(skill);
    }

    pub fn set_disabled(&self, names: Vec<String>) {
        *self.disabled.write().unwrap() = names.into_iter().collect();
    }

    pub fn is_disabled(&self, name: &str) -> bool {
        self.disabled.read().unwrap().contains(name)
    }

    pub fn find_by_trigger(&self, text: &str) -> Option<&dyn Skill> {
        let disabled = self.disabled.read().unwrap();
        self.skills
            .iter()
            .filter(|s| !disabled.contains(s.name()))
            .find(|s| {
                s.manifest()
                    .triggers
                    .iter()
                    .any(|t| text.contains(t.as_str()))
            })
            .map(|s| s.as_ref())
    }

    pub fn find_by_name(&self, name: &str) -> Option<&dyn Skill> {
        let disabled = self.disabled.read().unwrap();
        self.skills
            .iter()
            .filter(|s| !disabled.contains(s.name()))
            .find(|s| s.name() == name)
            .map(|s| s.as_ref())
    }

    pub fn list_all(&self) -> Vec<SkillInfo> {
        let disabled = self.disabled.read().unwrap();
        self.skills
            .iter()
            .filter(|s| !disabled.contains(s.name()))
            .map(|s| {
                let m = s.manifest();
                SkillInfo {
                    name: m.name.clone(),
                    description: m.description.clone(),
                    triggers: m.triggers.clone(),
                }
            })
            .collect()
    }

    pub async fn invoke(&self, name: &str, params: Value) -> Result<Value> {
        let skill = {
            let disabled = self.disabled.read().unwrap();
            self.skills
                .iter()
                .filter(|s| !disabled.contains(s.name()))
                .find(|s| s.name() == name)
                .ok_or_else(|| anyhow::anyhow!("skill '{}' not found", name))?
        };
        let permissions = skill.manifest().permissions.clone();
        openloom_sandbox::check_permissions(&permissions, name, &params)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        skill.invoke(params).await
    }

    pub fn list_all_skills(&self) -> Vec<SkillInfo> {
        self.skills
            .iter()
            .map(|s| {
                let m = s.manifest();
                SkillInfo {
                    name: m.name.clone(),
                    description: m.description.clone(),
                    triggers: m.triggers.clone(),
                }
            })
            .collect()
    }

    pub fn all_skills(&self) -> &[Box<dyn Skill>] {
        &self.skills
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// CLI Bridge — discovers executables on PATH
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliTool {
    pub name: String,
    pub description: String,
    pub binary: String,
}

pub struct CliBridge;

impl CliBridge {
    pub fn discover_path_tools() -> Vec<CliTool> {
        let common_tools = vec![
            ("gh", "GitHub CLI — manage issues, PRs, and repos"),
            ("git", "Version control system"),
            ("cargo", "Rust package manager"),
            ("npm", "Node.js package manager"),
            ("python", "Python interpreter"),
        ];
        common_tools
            .into_iter()
            .filter(|(binary, _)| Self::is_on_path(binary))
            .map(|(binary, desc)| CliTool {
                name: binary.to_string(),
                description: desc.to_string(),
                binary: binary.to_string(),
            })
            .collect()
    }

    fn is_on_path(binary: &str) -> bool {
        std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|dir| {
                let full = dir.join(binary);
                full.exists() || {
                    let with_ext = dir.join(format!("{}.exe", binary));
                    with_ext.exists()
                }
            })
        })
    }

    pub fn parse_help(binary: &str) -> Option<CliTool> {
        let output = std::process::Command::new(binary)
            .arg("--help")
            .output()
            .ok()?;
        let help_text = String::from_utf8_lossy(&output.stdout);
        let first_line = help_text.lines().next().unwrap_or(binary);
        Some(CliTool {
            name: binary.to_string(),
            description: first_line.to_string(),
            binary: binary.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TestSkill;
    #[async_trait::async_trait]
    impl Skill for TestSkill {
        fn name(&self) -> &str {
            "test"
        }
        fn manifest(&self) -> &SkillManifest {
            static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
            M.get_or_init(|| SkillManifest {
                name: "test".into(),
                description: "A test skill".into(),
                triggers: vec!["test".into(), "测试".into()],
                permissions: SkillPermissions::default(),
                min_engine_version: "0.1.0".into(),
            })
        }
        async fn invoke(&self, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            Ok(json!({"echo": params}))
        }
        fn context_md(&self) -> &str {
            "Test skill context"
        }
    }

    #[test]
    fn test_register_and_find_skill() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(TestSkill));
        let found = registry.find_by_trigger("运行测试");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "test");
    }

    #[test]
    fn test_list_skills() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(TestSkill));
        let skills = registry.list_all();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test");
    }

    #[test]
    fn test_find_nonexistent_skill() {
        let registry = SkillRegistry::new();
        let found = registry.find_by_trigger("不存在的技能");
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_invoke_skill() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(TestSkill));
        let result = registry
            .invoke("test", json!({"key": "value"}))
            .await
            .unwrap();
        assert_eq!(result["echo"]["key"], "value");
    }

    #[test]
    fn test_skill_permissions_default() {
        let perms = SkillPermissions::default();
        assert!(!perms.shell);
        assert!(!perms.subprocess);
        assert!(perms.fs_read.is_none());
        assert!(perms.fs_write.is_none());
        assert!(perms.network.is_none());
    }

    #[test]
    fn test_cli_bridge_discovers_cargo() {
        let tools = CliBridge::discover_path_tools();
        // cargo should be on PATH in a Rust development environment
        let has_cargo = tools.iter().any(|t| t.name == "cargo");
        assert!(has_cargo, "cargo should be discoverable on PATH");
    }
}
