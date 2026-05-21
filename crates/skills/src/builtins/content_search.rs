use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};
use std::io::BufRead;

pub struct ContentSearch;

#[async_trait::async_trait]
impl Skill for ContentSearch {
    fn name(&self) -> &str {
        "content_search"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "content_search".into(),
            description: "Search file contents with regex pattern".into(),
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
        let file_glob = params
            .get("glob")
            .and_then(|v| v.as_str())
            .unwrap_or("**/*");
        let max_results = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        if pattern.is_empty() {
            return Ok(json!({"error": "pattern is required (regex)"}));
        }

        let re = match regex::Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return Ok(json!({"error": format!("Invalid regex: {}", e)})),
        };

        let glob_pattern = format!("{}/{}", path, file_glob);
        let entries = match glob::glob(&glob_pattern) {
            Ok(e) => e,
            Err(e) => return Ok(json!({"error": format!("Invalid glob: {}", e)})),
        };

        let mut matches: Vec<Value> = Vec::new();

        for entry in entries.flatten() {
            if !entry.is_file() {
                continue;
            }
            let file = match std::fs::File::open(&entry) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let reader = std::io::BufReader::new(file);
            for (line_num, line) in reader.lines().enumerate() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break, // binary file
                };
                if re.is_match(&line) {
                    matches.push(json!({
                        "file": entry.display().to_string(),
                        "line": line_num + 1,
                        "content": if line.len() > 200 { format!("{}...", &line[..200]) } else { line }
                    }));
                    if matches.len() >= max_results {
                        return Ok(json!({
                            "matches": matches,
                            "count": matches.len(),
                            "truncated": true
                        }));
                    }
                }
            }
        }

        Ok(json!({
            "matches": matches,
            "count": matches.len(),
            "truncated": false
        }))
    }

    fn context_md(&self) -> &str {
        "Search file contents with regex. Params: {\"pattern\": \"fn main\", \"path\": \".\", \"glob\": \"**/*.rs\", \"max_results\": 50}. Returns matching lines with file paths."
    }
}
