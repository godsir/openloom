//! Built-in tools registered by default in the ToolRegistry.
//!
//! These provide essential capabilities without needing MCP servers:
//! shell, file_list, file_read, file_write, file_edit, content_search.

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{ToolDefinition, ToolProgress};
use tokio::sync::mpsc::UnboundedSender;

use crate::tool_context::ToolContext;
use crate::tool_registry::{AgentTool, ToolProvenance, ToolResult};

// ============================================================================
// Shell
// ============================================================================

pub struct ShellTool;

#[async_trait]
impl AgentTool for ShellTool {
    fn tool_name(&self) -> &str {
        "shell"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "shell".into(),
            description: "Execute a shell command and return its output. Supports PowerShell syntax on Windows (Get-ChildItem, $env:PATH, etc.) and bash syntax on Unix. Use for: listing files, reading file contents, searching with grep/Select-String, checking git status, running build commands. Default timeout is 60 seconds. Avoid destructive operations.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "cwd": { "type": "string", "description": "Working directory (optional)" },
                    "timeout": { "type": "integer", "description": "Timeout in seconds (default 60, max 300)" }
                },
                "required": ["command"]
            }),
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let command = arguments["command"].as_str().unwrap_or("");
        if command.is_empty() {
            return Ok(ToolResult {
                content: "No command provided.".into(),
                is_error: true,
                structured_content: None,
            });
        }

        let cwd = arguments["cwd"]
            .as_str()
            .map(|s| context.resolve_path(s))
            .or_else(|| context.workspace_path.as_ref().map(|ws| Path::new(ws).to_path_buf()));
        let timeout_secs = arguments["timeout"].as_u64().unwrap_or(60).min(300);

        // Use tokio::process::Command for async execution with timeout
        let child_result = if cfg!(windows) {
            // Prefer PowerShell over cmd.exe for better command support
            let pwsh = which_shell("pwsh").or_else(|| which_shell("powershell"));
            let default_cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
            let work_dir = cwd.as_deref().unwrap_or(&default_cwd);
            match pwsh {
                Some(shell_path) => {
                    tokio::process::Command::new(&shell_path)
                        .args(["-NoProfile", "-NonInteractive", "-Command", command])
                        .current_dir(work_dir)
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .spawn()
                }
                None => {
                    // Fallback to cmd.exe if PowerShell is not found
                    tokio::process::Command::new("cmd")
                        .args(["/c", command])
                        .current_dir(work_dir)
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .spawn()
                }
            }
        } else {
            let default_cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
            let work_dir = cwd.as_deref().unwrap_or(&default_cwd);
            tokio::process::Command::new("sh")
                .args(["-c", command])
                .current_dir(work_dir)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
        };

        let mut child = match child_result {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult {
                    content: format!("Shell execution failed: {}", e),
                    is_error: true,
                    structured_content: None,
                });
            }
        };

        // Wait with timeout
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);
        let wait_result = tokio::select! {
            result = child.wait() => Ok(result),
            _ = tokio::time::sleep(timeout_duration) => Err(()),
        };

        match wait_result {
            Ok(Ok(status)) => {
                // Process completed — collect output
                let stdout = if let Some(mut stdout_pipe) = child.stdout.take() {
                    let mut buf = Vec::new();
                    let _ = tokio::io::AsyncReadExt::read_to_end(&mut stdout_pipe, &mut buf).await;
                    buf
                } else {
                    Vec::new()
                };
                let stderr = if let Some(mut stderr_pipe) = child.stderr.take() {
                    let mut buf = Vec::new();
                    let _ = tokio::io::AsyncReadExt::read_to_end(&mut stderr_pipe, &mut buf).await;
                    buf
                } else {
                    Vec::new()
                };

                let is_error = !status.success();
                let stdout_str = String::from_utf8_lossy(&stdout);
                let stderr_str = String::from_utf8_lossy(&stderr);
                let mut content = if is_error {
                    format!("[FAIL] exit code {}\n", status.code().unwrap_or(-1))
                } else {
                    String::new()
                };
                if !stdout_str.is_empty() {
                    content.push_str(&stdout_str);
                }
                if !stderr_str.is_empty() {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str("[stderr]\n");
                    content.push_str(&stderr_str);
                }
                if content.is_empty() {
                    content = "[ok] Command executed on local machine — no errors.".to_string();
                }
                if content.len() > 65536 {
                    content =
                        format!("{}...\n[truncated at 64KB]", truncate_utf8(&content, 65536));
                }
                Ok(ToolResult {
                    content,
                    is_error,
                    structured_content: None,
                })
            }
            Ok(Err(e)) => Ok(ToolResult {
                content: format!("Shell execution failed: {}", e),
                is_error: true,
                structured_content: None,
            }),
            Err(_) => {
                // Timeout — kill the child process
                let _ = child.kill().await;
                Ok(ToolResult {
                    content: format!(
                        "[TIMEOUT] Command timed out after {} seconds and was killed.",
                        timeout_secs
                    ),
                    is_error: true,
                    structured_content: None,
                })
            }
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

/// Find a shell executable by name, checking common Windows paths.
/// Tries `pwsh` (PowerShell 7+) first, then falls back to `powershell` (5.1).
fn which_shell(name: &str) -> Option<String> {
    // On Windows, try common absolute paths first (fast, no process spawn)
    if cfg!(windows) {
        let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let candidates = if name == "pwsh" {
            vec![
                format!("C:\\Program Files\\PowerShell\\7\\{name}.exe"),
                format!("C:\\Program Files (x86)\\PowerShell\\7\\{name}.exe"),
            ]
        } else {
            vec![
                format!("{sysroot}\\System32\\WindowsPowerShell\\v1.0\\{name}.exe"),
                format!("{sysroot}\\SysWOW64\\WindowsPowerShell\\v1.0\\{name}.exe"),
            ]
        };
        for path in &candidates {
            if Path::new(path).exists() {
                return Some(path.clone());
            }
        }
    }

    // Fallback: check if it's directly available in PATH
    if cfg!(windows) {
        // Use `where` command on Windows
        if let Ok(output) = std::process::Command::new("where")
            .arg(name)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = path.lines().next() {
                    let trimmed = first_line.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }
    } else {
        // Use `which` on Unix
        if let Ok(output) = std::process::Command::new("which")
            .arg(name)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }
    None
}

// ============================================================================
// FileList
// ============================================================================

pub struct FileListTool;

#[async_trait]
impl AgentTool for FileListTool {
    fn tool_name(&self) -> &str {
        "file_list"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_list".into(),
            description:
                "List files and directories in a given path. Returns file names, sizes, and types."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list (absolute or relative)" },
                    "recursive": { "type": "boolean", "description": "If true, list recursively (max 3 levels)", "default": false }
                },
                "required": ["path"]
            }),
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let path_str = arguments["path"].as_str().unwrap_or(".");
        let recursive = arguments["recursive"].as_bool().unwrap_or(false);
        let path = context.resolve_path(path_str);

        if !path.exists() {
            return Ok(ToolResult {
                content: format!("Path does not exist: {}", path_str),
                is_error: true,
                structured_content: None,
            });
        }

        let mut result = format!("Contents of '{}':\n\n", path.display());
        match list_dir(&path, recursive, 0, if recursive { 3 } else { 1 }) {
            Ok(files) => {
                for (name, size, is_dir) in &files {
                    let prefix = if *is_dir { "[DIR] " } else { "[FILE]" };
                    result.push_str(&format!("{}  {}  {}\n", prefix, format_size(*size), name));
                }
                result.push_str(&format!("\n{} entries", files.len()));
                Ok(ToolResult {
                    content: result,
                    is_error: false,
                    structured_content: None,
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to list directory: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

fn list_dir(
    path: &Path,
    recursive: bool,
    depth: usize,
    max_depth: usize,
) -> Result<Vec<(String, u64, bool)>> {
    let mut entries = Vec::new();
    if depth >= max_depth {
        return Ok(entries);
    }

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
    if bytes < 1024 {
        format!("{:>5}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:>5.1}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:>5.1}M", bytes as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:>5.1}G", bytes as f64 / 1024.0 / 1024.0 / 1024.0)
    }
}

// ============================================================================
// FileRead
// ============================================================================

pub struct FileReadTool;

#[async_trait]
impl AgentTool for FileReadTool {
    fn tool_name(&self) -> &str {
        "file_read"
    }

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
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let path_str = arguments["path"].as_str().unwrap_or("");
        let max_lines = arguments["max_lines"].as_u64().unwrap_or(500) as usize;
        let path = context.resolve_path(path_str);

        if !path.exists() {
            return Ok(ToolResult {
                content: format!("File does not exist: {}", path_str),
                is_error: true,
                structured_content: None,
            });
        }
        if !path.is_file() {
            return Ok(ToolResult {
                content: format!("Not a file: {}", path_str),
                is_error: true,
                structured_content: None,
            });
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().take(max_lines).collect();
                let total = content.lines().count();
                let mut result = format!("File: {}\n", path.display());
                if total > max_lines {
                    result.push_str(&format!(
                        "(showing first {} of {} lines)\n\n",
                        max_lines, total
                    ));
                } else {
                    result.push('\n');
                }
                result.push_str(&lines.join("\n"));
                if result.len() > 65536 {
                    result = format!("{}...\n[truncated at 64KB]", &result[..65536]);
                }
                Ok(ToolResult {
                    content: result,
                    is_error: false,
                    structured_content: None,
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to read file: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

// ============================================================================
// FileWrite
// ============================================================================

pub struct FileWriteTool;

#[async_trait]
impl AgentTool for FileWriteTool {
    fn tool_name(&self) -> &str {
        "file_write"
    }

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
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let path_str = arguments["path"].as_str().unwrap_or("");
        let content = arguments["content"].as_str().unwrap_or("");

        if path_str.is_empty() {
            return Ok(ToolResult {
                content: "No path provided.".into(),
                is_error: true,
                structured_content: None,
            });
        }

        let path = context.resolve_path(path_str);
        let old_content = std::fs::read_to_string(&path).unwrap_or_default();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        match std::fs::write(&path, content) {
            Ok(_) => {
                let len = content.len();
                Ok(ToolResult {
                    content: format!("File written successfully: {} ({} bytes)", path_str, len),
                    is_error: false,
                    structured_content: Some(serde_json::json!({
                        "filePath": path_str,
                        "fileName": file_name,
                        "oldContent": old_content,
                        "newContent": content,
                    })),
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("Failed to write file: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

// ============================================================================
// ContentSearch (grep equivalent)
// ============================================================================

pub struct ContentSearchTool;

#[async_trait]
impl AgentTool for ContentSearchTool {
    fn tool_name(&self) -> &str {
        "content_search"
    }

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
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let pattern = arguments["pattern"].as_str().unwrap_or("");
        let search_path = arguments["path"].as_str().unwrap_or(".");
        let _file_glob = arguments["file_pattern"].as_str().unwrap_or("*");
        let max_results = arguments["max_results"].as_u64().unwrap_or(30) as usize;

        if pattern.is_empty() {
            return Ok(ToolResult {
                content: "No search pattern provided.".into(),
                is_error: true,
                structured_content: None,
            });
        }

        let resolved_path = context.resolve_path(search_path);
        // Always use recursive walker — more reliable than findstr on Windows
        match simple_content_search(&resolved_path, pattern, max_results) {
            Ok(results) => {
                if results.is_empty() {
                    Ok(ToolResult {
                        content: format!("No matches for '{}' in {}", pattern, search_path),
                        is_error: false,
                        structured_content: None,
                    })
                } else {
                    Ok(ToolResult {
                        content: format!(
                            "Found {} matches for '{}':\n\n{}",
                            results.len(),
                            pattern,
                            results.join("\n")
                        ),
                        is_error: false,
                        structured_content: None,
                    })
                }
            }
            Err(e) => Ok(ToolResult {
                content: format!("Search error: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

fn simple_content_search(path: &Path, pattern: &str, max_results: usize) -> Result<Vec<String>> {
    let mut results = Vec::new();
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir()
                && !p
                    .file_name()
                    .is_some_and(|n| n == ".git" || n == "node_modules" || n == "target")
            {
                if let Ok(sub) =
                    simple_content_search(&p, pattern, max_results.saturating_sub(results.len()))
                {
                    results.extend(sub);
                }
            } else if p.is_file()
                && let Ok(content) = std::fs::read_to_string(&p)
            {
                for (i, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&pattern.to_lowercase()) {
                        results.push(format!("{}:{}: {}", p.display(), i + 1, line));
                        if results.len() >= max_results {
                            return Ok(results);
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
    fn tool_name(&self) -> &str {
        "file_delete"
    }

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
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let path_str = arguments["path"].as_str().unwrap_or("");
        let path = context.resolve_path(path_str);
        if !path.exists() {
            return Ok(ToolResult {
                content: format!("Path does not exist: {}", path_str),
                is_error: true,
                structured_content: None,
            });
        }
        let result = if path.is_dir() {
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        };
        match result {
            Ok(_) => Ok(ToolResult {
                content: format!("Deleted: {}", path_str),
                is_error: false,
                structured_content: None,
            }),
            Err(e) => Ok(ToolResult {
                content: format!("Delete failed: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

// ============================================================================
// UseSkill — activates a loaded external skill (SKILL.md) by name
// ============================================================================

pub struct UseSkillTool {
    pub skill_bodies:
        std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
}

#[async_trait]
impl AgentTool for UseSkillTool {
    fn tool_name(&self) -> &str {
        "use_skill"
    }

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
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _context: &ToolContext,
    ) -> Result<ToolResult> {
        let name = arguments["skill_name"].as_str().unwrap_or("");
        if name.is_empty() {
            return Ok(ToolResult {
                content: "No skill name provided.".into(),
                is_error: true,
                structured_content: None,
            });
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

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// ============================================================================
// WebSearch — DuckDuckGo Lite search (no API key needed)
// ============================================================================

pub struct WebSearchTool;

#[async_trait]
impl AgentTool for WebSearchTool {
    fn tool_name(&self) -> &str {
        "web_search"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_search".into(),
            description: "Search the web using DuckDuckGo. Returns titles, URLs, and snippets for the given query. Use for finding current information, documentation, or answers.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "max_results": { "type": "integer", "description": "Max results (default 5)", "default": 5 }
                },
                "required": ["query"]
            }),
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _context: &ToolContext,
    ) -> Result<ToolResult> {
        let query = arguments["query"].as_str().unwrap_or("");
        let max_results = arguments["max_results"].as_u64().unwrap_or(5).min(10) as usize;

        if query.is_empty() {
            return Ok(ToolResult {
                content: "No search query provided.".into(),
                is_error: true,
                structured_content: None,
            });
        }

        let client = reqwest::Client::builder()
            .user_agent("openLoom/0.2")
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        // Use DuckDuckGo Lite (no JS, plain HTML, no API key)
        let url = format!("https://lite.duckduckgo.com/lite/?q={}", urlencoding(query));
        let html = client.get(&url).send().await?.text().await?;

        let results = parse_ddg_lite(&html, max_results);
        if results.is_empty() {
            Ok(ToolResult {
                content: format!("No results found for '{}'.", query),
                is_error: false,
                structured_content: None,
            })
        } else {
            let out = results
                .iter()
                .enumerate()
                .map(|(i, (title, snippet, link))| {
                    format!("{}. {}\n   {}\n   {}", i + 1, title, snippet, link)
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(ToolResult {
                content: out,
                is_error: false,
                structured_content: None,
            })
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

fn parse_ddg_lite(html: &str, max_results: usize) -> Vec<(String, String, String)> {
    let mut results = Vec::new();
    // DDG Lite format: <a href="URL" class="result-link">TITLE</a>
    //                 <span class="result-snippet">SNIPPET</span>
    let fragment = scraper::Html::parse_fragment(html);

    let link_sel = scraper::Selector::parse("a.result-link").unwrap();
    let snippet_sel = scraper::Selector::parse(".result-snippet").unwrap();

    let links: Vec<_> = fragment.select(&link_sel).collect();
    let snippets: Vec<_> = fragment.select(&snippet_sel).collect();

    let count = links.len().min(snippets.len()).min(max_results);
    for i in 0..count {
        let title = links[i]
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        let snippet = snippets[i]
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        let url = links[i].value().attr("href").unwrap_or("").to_string();
        if !title.is_empty() {
            results.push((title, snippet, url));
        }
    }
    results
}

fn urlencoding(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
            result.push(c);
        } else {
            for b in c.to_string().as_bytes() {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

// ============================================================================
// WebFetch — fetch and extract text from a URL
// ============================================================================

pub struct WebFetchTool;

#[async_trait]
impl AgentTool for WebFetchTool {
    fn tool_name(&self) -> &str {
        "web_fetch"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_fetch".into(),
            description: "Fetch and extract readable text content from a web page. Strips HTML, scripts, and styles. Use after web_search to read full content of a result.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Full URL of the page to fetch" },
                    "max_chars": { "type": "integer", "description": "Max characters to return (default 5000)", "default": 5000 }
                },
                "required": ["url"]
            }),
            tags: vec![],
        }
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _context: &ToolContext,
    ) -> Result<ToolResult> {
        let url = arguments["url"].as_str().unwrap_or("");
        let max_chars = arguments["max_chars"].as_u64().unwrap_or(5000).min(20000) as usize;

        if url.is_empty() {
            return Ok(ToolResult {
                content: "No URL provided.".into(),
                is_error: true,
                structured_content: None,
            });
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(ToolResult {
                content: format!("Invalid URL (must start with http/https): {}", url),
                is_error: true,
                structured_content: None,
            });
        }

        let client = reqwest::Client::builder()
            .user_agent("openLoom/0.2")
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let html = client.get(url).send().await?.text().await?;
        let text = extract_text(&html);

        if text.is_empty() {
            Ok(ToolResult {
                content: "Page returned no readable text content.".into(),
                is_error: false,
                structured_content: None,
            })
        } else if text.len() > max_chars {
            Ok(ToolResult {
                content: format!(
                    "{}...\n\n[truncated at {} chars, full page: {} chars]",
                    &text[..max_chars],
                    max_chars,
                    text.len()
                ),
                is_error: false,
                structured_content: None,
            })
        } else {
            Ok(ToolResult {
                content: text,
                is_error: false,
                structured_content: None,
            })
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

fn extract_text(html: &str) -> String {
    let document = scraper::Html::parse_document(html);

    // Collect text from <body> if available, otherwise from root
    let dom = if let Ok(body_sel) = scraper::Selector::parse("body") {
        document
            .select(&body_sel)
            .next()
            .unwrap_or(document.root_element())
    } else {
        document.root_element()
    };

    let text = dom.text().collect::<Vec<_>>().join(" ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
