use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};

pub struct FileManager;

#[async_trait::async_trait]
impl Skill for FileManager {
    fn name(&self) -> &str {
        "file-manager"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "file-manager".into(),
            description: "File management: read, write, list, and search files".into(),
            triggers: vec![
                "文件".into(),
                "文档".into(),
                "读写".into(),
                "保存".into(),
                "目录".into(),
                "文件夹".into(),
            ],
            permissions: SkillPermissions {
                fs_read: vec!["~".into()],
                fs_write: vec!["~".into()],
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        match action {
            "list" => {
                let entries: Vec<String> = std::fs::read_dir(path)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();
                Ok(json!({"files": entries}))
            }
            "read" => {
                let content = std::fs::read_to_string(path)?;
                Ok(json!({"content": content}))
            }
            "write" => {
                let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");
                std::fs::write(path, content)?;
                Ok(json!({"ok": true}))
            }
            _ => Ok(json!({"error": "unknown action", "available": ["list", "read", "write"]})),
        }
    }

    fn context_md(&self) -> &str {
        "File management skill: supports list/read/write operations with relative or absolute paths."
    }
}
