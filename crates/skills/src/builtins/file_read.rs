use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};
use std::io::BufRead;

pub struct FileRead;

#[async_trait::async_trait]
impl Skill for FileRead {
    fn name(&self) -> &str {
        "file_read"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "file_read".into(),
            description: "Read file contents with optional line range".into(),
            triggers: vec![],
            permissions: SkillPermissions {
                fs_read: Some(vec!["~".into(), ".".into()]),
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            return Ok(json!({"error": "path is required"}));
        }

        let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;

        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return Ok(json!({"error": format!("Cannot open file: {}", e)})),
        };

        let reader = std::io::BufReader::new(file);
        let mut lines: Vec<String> = Vec::new();
        let mut total_lines = 0;
        let mut truncated = false;

        for (i, line) in reader.lines().enumerate() {
            total_lines = i + 1;
            if i < offset {
                continue;
            }
            if lines.len() >= limit {
                truncated = true;
                break;
            }
            match line {
                Ok(l) => lines.push(format!("{}\t{}", i + 1, l)),
                Err(_) => {
                    return Ok(json!({"error": "Binary file or encoding error", "path": path}));
                }
            }
        }

        let content = lines.join("\n");
        Ok(json!({
            "content": content,
            "lines_count": total_lines,
            "truncated": truncated,
            "path": path
        }))
    }

    fn context_md(&self) -> &str {
        "Read file contents. Params: {\"path\": \"file.rs\", \"offset\": 0, \"limit\": 2000}. Returns numbered lines."
    }
}
