use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};

pub struct FileWrite;

#[async_trait::async_trait]
impl Skill for FileWrite {
    fn name(&self) -> &str {
        "file_write"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "file_write".into(),
            description: "Write content to a file (creates or overwrites)".into(),
            triggers: vec![],
            permissions: SkillPermissions {
                fs_write: Some(vec!["~".into(), ".".into()]),
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");

        if path.is_empty() {
            return Ok(json!({"error": "path is required"}));
        }

        if let Some(parent) = std::path::Path::new(path).parent()
            && !parent.as_os_str().is_empty()
        {
            let _ = std::fs::create_dir_all(parent);
        }

        match std::fs::write(path, content) {
            Ok(()) => Ok(json!({
                "ok": true,
                "path": path,
                "bytes_written": content.len()
            })),
            Err(e) => Ok(json!({"error": format!("Write failed: {}", e)})),
        }
    }

    fn context_md(&self) -> &str {
        "Write file contents. Params: {\"path\": \"file.rs\", \"content\": \"...\"}. Creates parent dirs if needed."
    }
}
