use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};

pub struct FileSearch;

#[async_trait::async_trait]
impl Skill for FileSearch {
    fn name(&self) -> &str {
        "file_search"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "file_search".into(),
            description: "Search for files by glob pattern".into(),
            triggers: vec![],
            permissions: SkillPermissions {
                fs_read: Some(vec!["~".into(), ".".into()]),
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let pattern = params.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        if pattern.is_empty() {
            return Ok(json!({"error": "pattern is required (e.g. \"**/*.rs\")"}));
        }

        let full_pattern = if pattern.starts_with('/') || pattern.starts_with("C:") {
            pattern.to_string()
        } else {
            format!("{}/{}", path, pattern)
        };

        let mut files: Vec<String> = Vec::new();
        match glob::glob(&full_pattern) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    files.push(entry.display().to_string());
                    if files.len() >= 200 {
                        break;
                    }
                }
            }
            Err(e) => return Ok(json!({"error": format!("Invalid glob pattern: {}", e)})),
        }

        Ok(json!({
            "files": files,
            "count": files.len(),
            "pattern": pattern,
            "truncated": files.len() >= 200
        }))
    }

    fn context_md(&self) -> &str {
        "Search files by glob pattern. Params: {\"pattern\": \"**/*.rs\", \"path\": \".\"}. Returns list of matching file paths."
    }
}
