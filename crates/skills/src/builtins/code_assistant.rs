use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};

pub struct CodeAssistant;

#[async_trait::async_trait]
impl Skill for CodeAssistant {
    fn name(&self) -> &str {
        "code-assistant"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "code-assistant".into(),
            description: "Code assistance: write, debug, refactor, run tests".into(),
            triggers: vec![
                "代码".into(),
                "编程".into(),
                "写".into(),
                "实现".into(),
                "修复".into(),
                "bug".into(),
                "测试".into(),
                "编译".into(),
                "运行".into(),
                "git".into(),
            ],
            permissions: SkillPermissions {
                shell: true,
                subprocess: true,
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("analyze");
        match action {
            "run_test" => {
                let output = std::process::Command::new("cargo").args(["test"]).output();
                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        Ok(json!({"output": stdout, "success": out.status.success()}))
                    }
                    Err(e) => Ok(json!({"error": e.to_string()})),
                }
            }
            _ => Ok(json!({
                "note": "CodeAssistant: provides code analysis, test running, and git operations",
                "available_actions": ["run_test", "format", "git_status"]
            })),
        }
    }

    fn context_md(&self) -> &str {
        "Code assistance skill: run cargo test/fmt/clippy, check git status."
    }
}
