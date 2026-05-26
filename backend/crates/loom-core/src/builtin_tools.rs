//! Built-in tools registered by default in the ToolRegistry.
//!
//! These provide essential capabilities without needing MCP servers:
//! shell, file_list, file_read, file_write, file_edit, content_search.

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{ToolDefinition, ToolProgress};
use tokio::sync::mpsc::UnboundedSender;

use crate::tool_registry::{AgentTool, ToolProvenance, ToolResult};

// ============================================================================
// Shell
// ============================================================================

pub struct ShellTool;

#[async_trait]
impl AgentTool for ShellTool {
    fn tool_name(&self) -> &str { "shell" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "shell".into(),
            description: "Execute a shell command and return its output. Use for: listing files, reading file contents, searching with grep, checking git status, running build commands. Avoid destructive operations.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "cwd": { "type": "string", "description": "Working directory (optional)" }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>) -> Result<ToolResult> {
        let command = arguments["command"].as_str().unwrap_or("");
        if command.is_empty() {
            return Ok(ToolResult { content: "No command provided.".into(), is_error: true, structured_content: None });
        }

        let cwd = arguments["cwd"].as_str().map(Path::new);

        // On Windows, wrap in cmd /c
        let output = if cfg!(windows) {
            std::process::Command::new("cmd")
                .args(["/c", command])
                .current_dir(cwd.unwrap_or(Path::new(".")))
                .output()
        } else {
            std::process::Command::new("sh")
                .args(["-c", command])
                .current_dir(cwd.unwrap_or(Path::new(".")))
                .output()
        };

        match output {
            Ok(out) => {
                let is_error = !out.status.success();
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut content = if is_error { format!("[FAIL] exit code {}\n", out.status.code().unwrap_or(-1)) } else { String::new() };
                if !stdout.is_empty() { content.push_str(&stdout); }
                if !stderr.is_empty() {
                    if !content.is_empty() { content.push('\n'); }
                    content.push_str("[stderr]\n");
                    content.push_str(&stderr);
                }
                if content.is_empty() { content = "[ok] Command executed on local machine — no errors.".to_string(); }
                if content.len() > 65536 {
                    content = format!("{}...\n[truncated at 64KB]", truncate_utf8(&content, 65536));
                }
                Ok(ToolResult { content, is_error, structured_content: None })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Shell execution failed: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// FileList
// ============================================================================

pub struct FileListTool;

#[async_trait]
impl AgentTool for FileListTool {
    fn tool_name(&self) -> &str { "file_list" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_list".into(),
            description: "List files and directories in a given path. Returns file names, sizes, and types.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list (absolute or relative)" },
                    "recursive": { "type": "boolean", "description": "If true, list recursively (max 3 levels)", "default": false }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>) -> Result<ToolResult> {
        let path_str = arguments["path"].as_str().unwrap_or(".");
        let recursive = arguments["recursive"].as_bool().unwrap_or(false);
        let path = Path::new(path_str);

        if !path.exists() {
            return Ok(ToolResult {
                content: format!("Path does not exist: {}", path_str),
                is_error: true, structured_content: None,
            });
        }

        let mut result = format!("Contents of '{}':\n\n", path.display());
        match list_dir(path, recursive, 0, if recursive { 3 } else { 1 }) {
            Ok(files) => {
                for (name, size, is_dir) in &files {
                    let prefix = if *is_dir { "[DIR] " } else { "[FILE]" };
                    result.push_str(&format!("{}  {}  {}\n", prefix, format_size(*size), name));
                }
                result.push_str(&format!("\n{} entries", files.len()));
                Ok(ToolResult { content: result, is_error: false, structured_content: None })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to list directory: {}", e),
                is_error: true, structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

fn list_dir(path: &Path, recursive: bool, depth: usize, max_depth: usize) -> Result<Vec<(String, u64, bool)>> {
    let mut entries = Vec::new();
    if depth >= max_depth { return Ok(entries); }

    let dir = std::fs::read_dir(path)?;
    for entry in dir {
        let entry = entry?;
        let meta = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        let prefix = if depth > 0 {
            "  ".repeat(depth) + "├─ "
        } else {
            String::new()
        };
        entries.push((format!("{}{}", prefix, name), meta.len(), meta.is_dir()));

        if recursive && meta.is_dir() {
            let sub = list_dir(&entry.path(), true, depth + 1, max_depth)?;
            entries.extend(sub);
        }
    }
    Ok(entries)
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{:>5}B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:>5.1}K", bytes as f64 / 1024.0) }
    else if bytes < 1024 * 1024 * 1024 { format!("{:>5.1}M", bytes as f64 / 1024.0 / 1024.0) }
    else { format!("{:>5.1}G", bytes as f64 / 1024.0 / 1024.0 / 1024.0) }
}

// ============================================================================
// FileRead
// ============================================================================

pub struct FileReadTool;

#[async_trait]
impl AgentTool for FileReadTool {
    fn tool_name(&self) -> &str { "file_read" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_read".into(),
            description: "Read the contents of a file. Returns the file content as text. Supports text files, code files, configs, etc. Use for inspecting file contents.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute or relative path to the file" },
                    "max_lines": { "type": "integer", "description": "Maximum lines to return (default 500)", "default": 500 }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>) -> Result<ToolResult> {
        let path_str = arguments["path"].as_str().unwrap_or("");
        let max_lines = arguments["max_lines"].as_u64().unwrap_or(500) as usize;
        let path = Path::new(path_str);

        if !path.exists() {
            return Ok(ToolResult {
                content: format!("File does not exist: {}", path_str),
                is_error: true, structured_content: None,
            });
        }
        if !path.is_file() {
            return Ok(ToolResult {
                content: format!("Not a file: {}", path_str),
                is_error: true, structured_content: None,
            });
        }

        match std::fs::read_to_string(path) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().take(max_lines).collect();
                let total = content.lines().count();
                let mut result = format!("File: {}\n", path.display());
                if total > max_lines {
                    result.push_str(&format!("(showing first {} of {} lines)\n\n", max_lines, total));
                } else {
                    result.push('\n');
                }
                result.push_str(&lines.join("\n"));
                if result.len() > 65536 {
                    result = format!("{}...\n[truncated at 64KB]", &result[..65536]);
                }
                Ok(ToolResult { content: result, is_error: false, structured_content: None })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to read file: {}", e),
                is_error: true, structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// FileWrite
// ============================================================================

pub struct FileWriteTool;

#[async_trait]
impl AgentTool for FileWriteTool {
    fn tool_name(&self) -> &str { "file_write" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_write".into(),
            description: "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Use with caution.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file to write" },
                    "content": { "type": "string", "description": "Content to write to the file" }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>) -> Result<ToolResult> {
        let path_str = arguments["path"].as_str().unwrap_or("");
        let content = arguments["content"].as_str().unwrap_or("");

        if path_str.is_empty() {
            return Ok(ToolResult { content: "No path provided.".into(), is_error: true, structured_content: None });
        }

        match std::fs::write(Path::new(path_str), content) {
            Ok(_) => {
                let len = content.len();
                Ok(ToolResult {
                    content: format!("File written successfully: {} ({} bytes)", path_str, len),
                    is_error: false, structured_content: None,
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to write file: {}", e),
                is_error: true, structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// ContentSearch (grep equivalent)
// ============================================================================

pub struct ContentSearchTool;

#[async_trait]
impl AgentTool for ContentSearchTool {
    fn tool_name(&self) -> &str { "content_search" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "content_search".into(),
            description: "Search for a text pattern in files within a directory. Like grep. Returns matching file paths and line content.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Text or regex pattern to search for" },
                    "path": { "type": "string", "description": "Directory to search in (default: current dir)", "default": "." },
                    "file_pattern": { "type": "string", "description": "Glob pattern for files to search (e.g., '*.rs', '*.md')", "default": "*" },
                    "max_results": { "type": "integer", "description": "Max results to return", "default": 30 }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>) -> Result<ToolResult> {
        let pattern = arguments["pattern"].as_str().unwrap_or("");
        let search_path = arguments["path"].as_str().unwrap_or(".");
        let _file_glob = arguments["file_pattern"].as_str().unwrap_or("*");
        let max_results = arguments["max_results"].as_u64().unwrap_or(30) as usize;

        if pattern.is_empty() {
            return Ok(ToolResult { content: "No search pattern provided.".into(), is_error: true, structured_content: None });
        }

        // Always use recursive walker — more reliable than findstr on Windows
        match simple_content_search(Path::new(search_path), pattern, max_results) {
            Ok(results) => {
                if results.is_empty() {
                    Ok(ToolResult { content: format!("No matches for '{}' in {}", pattern, search_path), is_error: false, structured_content: None })
                } else {
                    Ok(ToolResult { content: format!("Found {} matches for '{}':\n\n{}", results.len(), pattern, results.join("\n")), is_error: false, structured_content: None })
                }
            }
            Err(e) => Ok(ToolResult { content: format!("Search error: {}", e), is_error: true, structured_content: None }),
        }
    }

    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

fn simple_content_search(path: &Path, pattern: &str, max_results: usize) -> Result<Vec<String>> {
    let mut results = Vec::new();
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() && !p.file_name().map_or(false, |n| n == ".git" || n == "node_modules" || n == "target") {
                if let Ok(sub) = simple_content_search(&p, pattern, max_results.saturating_sub(results.len())) {
                    results.extend(sub);
                }
            } else if p.is_file() {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    for (i, line) in content.lines().enumerate() {
                        if line.to_lowercase().contains(&pattern.to_lowercase()) {
                            results.push(format!("{}:{}: {}", p.display(), i + 1, line));
                            if results.len() >= max_results { return Ok(results); }
                        }
                    }
                }
            }
        }
    }
    Ok(results)
}

// ============================================================================
// FileDelete
// ============================================================================

pub struct FileDeleteTool;

#[async_trait]
impl AgentTool for FileDeleteTool {
    fn tool_name(&self) -> &str { "file_delete" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_delete".into(),
            description: "Delete a file or empty directory at the given path. Use with caution — this is irreversible.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute or relative path to the file or empty directory to delete" }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>) -> Result<ToolResult> {
        let path_str = arguments["path"].as_str().unwrap_or("");
        let path = std::path::Path::new(path_str);
        if !path.exists() {
            return Ok(ToolResult { content: format!("Path does not exist: {}", path_str), is_error: true, structured_content: None });
        }
        let result = if path.is_dir() {
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        };
        match result {
            Ok(_) => Ok(ToolResult { content: format!("Deleted: {}", path_str), is_error: false, structured_content: None }),
            Err(e) => Ok(ToolResult { content: format!("Delete failed: {}", e), is_error: true, structured_content: None }),
        }
    }

    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// UseSkill — activates a loaded external skill (SKILL.md) by name
// ============================================================================

pub struct UseSkillTool {
    pub skill_bodies: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
}

#[async_trait]
impl AgentTool for UseSkillTool {
    fn tool_name(&self) -> &str { "use_skill" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "use_skill".into(),
            description: "Activate an available skill by name to get its full instructions. Use when a task requires a skill you know is available (e.g. from the system prompt skills list).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill_name": { "type": "string", "description": "Name of the skill to activate" }
                },
                "required": ["skill_name"]
            }),
        }
    }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>) -> Result<ToolResult> {
        let name = arguments["skill_name"].as_str().unwrap_or("");
        if name.is_empty() {
            return Ok(ToolResult { content: "No skill name provided.".into(), is_error: true, structured_content: None });
        }
        let bodies = self.skill_bodies.read().await;
        if let Some(body) = bodies.get(name) {
            Ok(ToolResult {
                content: format!("## Skill: {}\n\n{}", name, body),
                is_error: false,
                structured_content: None,
            })
        } else {
            let available: Vec<&String> = bodies.keys().collect();
            Ok(ToolResult {
                content: format!("Skill '{}' not found. Available: {:?}", name, available),
                is_error: true,
                structured_content: None,
            })
        }
    }

    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes { return s; }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) { end -= 1; }
    &s[..end]
}
