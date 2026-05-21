use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};

pub struct FileEdit;

#[async_trait::async_trait]
impl Skill for FileEdit {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "file_edit".into(),
            description: "Edit file with search-and-replace (exact string match)".into(),
            triggers: vec![],
            permissions: SkillPermissions {
                fs_read: Some(vec!["~".into(), ".".into()]),
                fs_write: Some(vec!["~".into(), ".".into()]),
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let old_string = params
            .get("old_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let new_string = params
            .get("new_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let replace_all = params
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if path.is_empty() {
            return Ok(json!({"error": "path is required"}));
        }
        if old_string.is_empty() {
            return Ok(json!({"error": "old_string is required"}));
        }
        if old_string == new_string {
            return Ok(json!({"error": "old_string and new_string are the same"}));
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return Ok(json!({"error": format!("Cannot read file: {}", e)})),
        };

        if !content.contains(old_string) {
            return Ok(json!({
                "error": "old_string not found in file",
                "path": path,
                "hint": "Make sure old_string matches exactly (including whitespace and indentation)"
            }));
        }

        let occurrences = content.matches(old_string).count();
        if occurrences > 1 && !replace_all {
            return Ok(json!({
                "error": format!("old_string has {} occurrences. Use replace_all:true or provide more context to make it unique.", occurrences),
                "path": path,
                "occurrences": occurrences
            }));
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        // Generate simple diff
        let diff = generate_diff(old_string, new_string, path);

        match std::fs::write(path, &new_content) {
            Ok(()) => Ok(json!({
                "ok": true,
                "path": path,
                "replacements": if replace_all { occurrences } else { 1 },
                "diff": diff
            })),
            Err(e) => Ok(json!({"error": format!("Write failed: {}", e)})),
        }
    }

    fn context_md(&self) -> &str {
        "Edit file with exact string replacement. Params: {\"path\": \"...\", \"old_string\": \"...\", \"new_string\": \"...\", \"replace_all\": false}. old_string must match exactly."
    }
}

fn generate_diff(old: &str, new: &str, path: &str) -> String {
    let mut diff = format!("--- a/{}\n+++ b/{}\n", path, path);
    for line in old.lines() {
        diff.push_str(&format!("-{}\n", line));
    }
    for line in new.lines() {
        diff.push_str(&format!("+{}\n", line));
    }
    diff
}
