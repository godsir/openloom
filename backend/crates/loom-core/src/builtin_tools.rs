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
            .or_else(|| {
                context
                    .workspace_path
                    .as_ref()
                    .map(|ws| Path::new(ws).to_path_buf())
            });
        let timeout_secs = arguments["timeout"].as_u64().unwrap_or(60).min(300);

        // Resolve actual working directory once, used for both sandbox check and shell execution
        let default_cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        let work_dir = cwd.as_deref().unwrap_or(&default_cwd);

        // Sandbox guard: check exec permission
        if let Some(ref guard) = context.sandbox
            && let Err(reason) = guard.check_exec(work_dir)
        {
            return Ok(ToolResult {
                content: format!("沙盒拒绝: {}", reason),
                is_error: true,
                structured_content: None,
            });
        }

        // Use tokio::process::Command for async execution with timeout
        let child_result = if cfg!(windows) {
            // Prefer PowerShell over cmd.exe for better command support
            let pwsh = which_shell("pwsh").or_else(|| which_shell("powershell"));
            match pwsh {
                Some(shell_path) => tokio::process::Command::new(&shell_path)
                    .args(["-NoProfile", "-NonInteractive", "-Command", command])
                    .current_dir(work_dir)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn(),
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
            let default_cwd =
                std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
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
                    content = format!("{}...\n[truncated at 64KB]", truncate_utf8(&content, 65536));
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
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout);
            if let Some(first_line) = path.lines().next() {
                let trimmed = first_line.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
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
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
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

    fn supports_parallel(&self) -> bool { true }

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

        // Sandbox guard: check read permission
        if let Some(ref guard) = context.sandbox
            && let Err(reason) = guard.check_read(&path)
        {
            return Ok(ToolResult {
                content: format!("沙盒拒绝: {}", reason),
                is_error: true,
                structured_content: None,
            });
        }

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
                for (name, size, is_dir, mtime) in &files {
                    let prefix = if *is_dir { "[DIR] " } else { "[FILE]" };
                    result.push_str(&format!("{}  {}  {}  {}\n", prefix, format_size(*size), mtime, name));
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
) -> Result<Vec<(String, u64, bool, String)>> {
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
        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(
                modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64, 0
            ).map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "unknown".to_string());
            entries.push((format!("{}{}", prefix, name), meta.len(), meta.is_dir(), dt));

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

    fn supports_parallel(&self) -> bool { true }

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

        // Sandbox guard: check read permission
        if let Some(ref guard) = context.sandbox
            && let Err(reason) = guard.check_read(&path)
        {
            return Ok(ToolResult {
                content: format!("沙盒拒绝: {}", reason),
                is_error: true,
                structured_content: None,
            });
        }

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
                context.record_read(path.clone());
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
                    result = format!("{}...\n[truncated at 64KB]", truncate_utf8(&result, 65536));
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
            let msg = if content.is_empty() {
                format!(
                    "file_write called with no arguments — both 'path' and 'content' are missing. Usage: file_write(path=\"/path/to/file\", content=\"...\"). Received: {}",
                    arguments
                )
            } else {
                format!(
                    "No path provided. Usage: file_write(path=\"/path/to/file\", content=\"...\"). Received: {}",
                    arguments
                )
            };
            return Ok(ToolResult {
                content: msg,
                is_error: true,
                structured_content: None,
            });
        }

        let path = context.resolve_path(path_str);

        // Sandbox guard: check write permission
        if let Some(ref guard) = context.sandbox
            && let Err(reason) = guard.check_write(&path)
        {
            return Ok(ToolResult {
                content: format!("沙盒拒绝: {}", reason),
                is_error: true,
                structured_content: None,
            });
        }
        // Read-before-edit guard: existing files must have been read recently
        if path.exists() && !context.was_recently_read(&path) {
            return Ok(ToolResult {
                content: format!("Read-before-edit guard: '{}' not read. Use file_read first.", path_str),
                is_error: true,
                structured_content: None,
            });
        }
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
// FileEdit — precise text replacement
// ============================================================================

pub struct FileEditTool;

#[async_trait]
impl AgentTool for FileEditTool {
    fn tool_name(&self) -> &str { "file_edit" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_edit".into(),
            description: "Edit a file using exact text replacement. Supports single edit (oldText/newText) or batch edits (edits array). Edits applied back-to-front. oldText must be unique. Use file_read first.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type":"string","description":"Path to edit"},
                    "oldText": {"type":"string","description":"Text to replace"},
                    "newText": {"type":"string","description":"Replacement"},
                    "edits": {"type":"array","items":{"type":"object","properties":{"oldText":{"type":"string"},"newText":{"type":"string"}},"required":["oldText","newText"]}}
                },
                "required": ["path"]
            }),
            tags: vec![],
        }
    }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>, context: &ToolContext) -> Result<ToolResult> {
        let path_str = arguments["path"].as_str().unwrap_or("");
        if path_str.is_empty() {
            return Ok(ToolResult { content: "No path provided.".into(), is_error: true, structured_content: None });
        }
        let path = context.resolve_path(path_str);
        if let Some(ref guard) = context.sandbox && let Err(reason) = guard.check_write(&path) {
            return Ok(ToolResult { content: format!("Sandbox: {}", reason), is_error: true, structured_content: None });
        }
        if path.exists() && !context.was_recently_read(&path) {
            return Ok(ToolResult { content: format!("Read-before-edit guard: '{}' not read. Use file_read first.", path_str), is_error: true, structured_content: None });
        }
        let old_content = match std::fs::read_to_string(&path) { Ok(c) => c, Err(e) => return Ok(ToolResult { content: format!("Read failed: {}", e), is_error: true, structured_content: None }) };
        let normalized = old_content.replace("\r\n", "\n");
        let edits_raw: Vec<(&str, &str)> = if let Some(arr) = arguments["edits"].as_array() {
            if arr.is_empty() { return Ok(ToolResult { content: "Empty edits array.".into(), is_error: true, structured_content: None }); }
            arr.iter().map(|e| (e["oldText"].as_str().unwrap_or(""), e["newText"].as_str().unwrap_or(""))).collect()
        } else {
            let ot = arguments["oldText"].as_str().unwrap_or("");
            if ot.is_empty() { return Ok(ToolResult { content: "No oldText provided.".into(), is_error: true, structured_content: None }); }
            vec![(ot, arguments["newText"].as_str().unwrap_or(""))]
        };
        let edits: Vec<(String, String)> = edits_raw.into_iter().map(|(o,n)| (o.replace("\r\n", "\n"), n.replace("\r\n", "\n"))).collect();
        struct EP { idx: usize, pos: usize, new: String }
        let mut eps: Vec<EP> = Vec::new();
        for (i, (old, new)) in edits.iter().enumerate() {
            let c = normalized.matches(old.as_str()).count();
            if c == 0 { return Ok(ToolResult { content: format!("Edit #{}: oldText not found.", i+1), is_error: true, structured_content: None }); }
            if c > 1 { return Ok(ToolResult { content: format!("Edit #{}: oldText appears {} times.", i+1, c), is_error: true, structured_content: None }); }
            eps.push(EP { idx: i, pos: normalized.find(old.as_str()).unwrap(), new: new.clone() });
        }
        eps.sort_by(|a,b| b.pos.cmp(&a.pos));
        for i in 0..eps.len() { for j in i+1..eps.len() { if eps[j].pos + edits[eps[j].idx].0.len() > eps[i].pos { return Ok(ToolResult { content: format!("Overlap: edits #{} and #{}", eps[i].idx+1, eps[j].idx+1), is_error: true, structured_content: None }); } } }
        let mut result = normalized.clone();
        for ep in &eps { result.replace_range(ep.pos..ep.pos+edits[ep.idx].0.len(), &ep.new); }
        if old_content.contains("\r\n") { result = result.replace("\n", "\r\n"); }
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        match std::fs::write(&path, &result) {
            Ok(_) => Ok(ToolResult { content: format!("Edited: {} ({} changes)", path_str, eps.len()), is_error: false, structured_content: Some(serde_json::json!({"filePath":path_str,"fileName":file_name,"oldContent":old_content,"newContent":result})) }),
            Err(e) => Ok(ToolResult { content: format!("Write failed: {}", e), is_error: true, structured_content: None }),
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
    fn tool_name(&self) -> &str {
        "content_search"
    }

    fn supports_parallel(&self) -> bool { true }

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
        let file_pattern = arguments["file_pattern"].as_str().unwrap_or("*");
        let max_results = arguments["max_results"].as_u64().unwrap_or(30) as usize;

        if pattern.is_empty() {
            return Ok(ToolResult {
                content: "No search pattern provided.".into(),
                is_error: true,
                structured_content: None,
            });
        }

        let resolved_path = context.resolve_path(search_path);

        // Sandbox guard: check read permission on the search directory
        if let Some(ref guard) = context.sandbox
            && let Err(reason) = guard.check_read(&resolved_path)
        {
            return Ok(ToolResult {
                content: format!("沙盒拒绝: {}", reason),
                is_error: true,
                structured_content: None,
            });
        }
        // Always use recursive walker — more reliable than findstr on Windows
        match simple_content_search(&resolved_path, pattern, file_pattern, max_results) {
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

fn file_name_matches(path: &Path, pattern: &str) -> bool {
    if pattern == "*" || pattern.is_empty() {
        return true;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.to_lowercase().contains(&pattern.to_lowercase()))
}

fn simple_content_search(path: &Path, pattern: &str, file_pattern: &str, max_results: usize) -> Result<Vec<String>> {
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
                    simple_content_search(&p, pattern, file_pattern, max_results.saturating_sub(results.len()))
                {
                    results.extend(sub);
                }
            } else if p.is_file()
                && file_name_matches(&p, file_pattern)
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

        // Sandbox guard: check write permission (delete is a destructive write)
        if let Some(ref guard) = context.sandbox
            && let Err(reason) = guard.check_write(&path)
        {
            return Ok(ToolResult {
                content: format!("沙盒拒绝: {}", reason),
                is_error: true,
                structured_content: None,
            });
        }
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
    pub skill_state: std::sync::Arc<tokio::sync::RwLock<loom_skills::SkillState>>,
}

#[async_trait]
impl AgentTool for UseSkillTool {
    fn tool_name(&self) -> &str {
        "use_skill"
    }

    fn tool_definition(&self) -> ToolDefinition {
        // Build dynamic description with the full available-skills catalogue
        // so the LLM can semantically match user intent → skill name without
        // needing to scan the separate "Available Skills" system prompt section.
        let skill_list = if let Ok(state) = self.skill_state.try_read() {
            if state.summaries.is_empty() {
                String::new()
            } else {
                state
                    .summaries
                    .iter()
                    .map(|s| format!("- {}: {}", s.name, s.description))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        } else {
            // Lock contested (rare — only during skill reload); fall back to
            // a static description so we never block the calling thread.
            String::new()
        };

        let description = if skill_list.is_empty() {
            "Load a skill's full instructions into context. No skills are currently installed — call with an empty skill_name or \"list\" to confirm.".into()
        } else {
            format!(
                "Load a skill's full instructions into context by name.\n\nAvailable skills:\n{}\n\nWhen the user's request matches a skill above, call use_skill with the exact skill_name FIRST, then follow the loaded instructions. Do NOT attempt skill-related tasks with only general knowledge when a matching skill exists. Pass an empty name or \"list\" to see this list again.",
                skill_list
            )
        };

        ToolDefinition {
            name: "use_skill".into(),
            description,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill_name": { "type": "string", "description": "Exact skill name from the list in the tool description" }
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
        if name.is_empty() || name == "list" {
            let state = self.skill_state.read().await;
            let available: Vec<&String> = state.bodies.keys().collect();
            if available.is_empty() {
                return Ok(ToolResult {
                    content: "没有安装任何技能。你可以在技能市场导入技能。".into(),
                    is_error: false,
                    structured_content: None,
                });
            }
            return Ok(ToolResult {
                content: format!(
                    "可用技能: {}",
                    available
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                is_error: false,
                structured_content: None,
            });
        }
        let state = self.skill_state.read().await;
        if let Some(body) = state.bodies.get(name) {
            let content = format!(
                "## Skill: {}\n\n{}\n\n---\nThe skill instructions above are now loaded and active. Do NOT call use_skill for \"{}\" again in this conversation — the skill is already loaded and its instructions persist.",
                name, body, name
            );
            Ok(ToolResult {
                content,
                is_error: false,
                structured_content: Some(serde_json::json!({
                    "skill_name": name,
                    "skill_body": body,
                    "skill_activated": true,
                })),
            })
        } else {
            let available: Vec<&String> = state.bodies.keys().collect();
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
// AskUser — LLM asks user clarifying question
// ============================================================================

pub struct AskUserTool;

#[async_trait]
impl AgentTool for AskUserTool {
    fn tool_name(&self) -> &str { "ask_user" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ask_user".into(),
            description: "Ask the user a clarifying question when their request is ambiguous. Use when you need more information — don't guess.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": {"type":"string","description":"The clarifying question"},
                    "options": {"type":"array","items":{"type":"string"},"description":"Optional choices"}
                },
                "required": ["question"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool { false }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>, _context: &ToolContext) -> Result<ToolResult> {
        let question = arguments["question"].as_str().unwrap_or("");
        let options: Vec<String> = arguments["options"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        let mut content = format!("? {}", question);
        if !options.is_empty() {
            content.push_str("\n\nOptions:");
            for (i, o) in options.iter().enumerate() { content.push_str(&format!("\n  {}. {}", i+1, o)); }
        }
        Ok(ToolResult { content, is_error: false, structured_content: Some(serde_json::json!({"question":question,"options":options})) })
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// GlobTool — real glob pattern file matching
// ============================================================================

pub struct GlobTool;

#[async_trait]
impl AgentTool for GlobTool {
    fn tool_name(&self) -> &str { "file_glob" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_glob".into(),
            description: "Find files matching a glob pattern (e.g. 'src/**/*.rs'). Returns paths with sizes. Use instead of file_list when you know the pattern.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {"type":"string","description":"Glob pattern. Supports *, **, ?, [abc]."},
                    "path": {"type":"string","description":"Base directory (default: workspace)"},
                    "max_results": {"type":"integer","description":"Max results (default 200)"}
                },
                "required": ["pattern"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool { true }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>, context: &ToolContext) -> Result<ToolResult> {
        let pattern = arguments["pattern"].as_str().unwrap_or("");
        let base = arguments["path"].as_str().unwrap_or(".");
        let max_results = arguments["max_results"].as_u64().unwrap_or(200) as usize;
        if pattern.is_empty() { return Ok(ToolResult { content: "No pattern.".into(), is_error: true, structured_content: None }); }
        let resolved = context.resolve_path(base);
        if let Some(ref guard) = context.sandbox && let Err(reason) = guard.check_read(&resolved) {
            return Ok(ToolResult { content: format!("Sandbox: {}", reason), is_error: true, structured_content: None });
        }
        context.record_read(resolved.clone());
        // Use glob crate
        let full_pattern = format!("{}/{}", resolved.display(), pattern);
        let mut results = Vec::new();
        match glob::glob(&full_pattern) {
            Ok(paths) => {
                for entry in paths.flatten() {
                    if results.len() >= max_results { break; }
                    if let Ok(meta) = std::fs::metadata(&entry) {
                        let rel = entry.strip_prefix(&resolved).unwrap_or(&entry).display().to_string();
                        results.push((rel, meta.len()));
                    }
                }
            }
            Err(e) => { return Ok(ToolResult { content: format!("Glob error: {}", e), is_error: true, structured_content: None }); }
        }
        if results.is_empty() { return Ok(ToolResult { content: format!("No files matching '{}'", pattern), is_error: false, structured_content: None }); }
        let mut out = format!("Found {} files matching '{}':\n", results.len(), pattern);
        for (path, size) in &results { out.push_str(&format!("  {}  {}\n", format_size(*size), path)); }
        if results.len() >= max_results { out.push_str(&format!("... truncated at {}\n", max_results)); }
        Ok(ToolResult { content: out, is_error: false, structured_content: None })
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// FindTool — find files by name
// ============================================================================

pub struct FindTool;

#[async_trait]
impl AgentTool for FindTool {
    fn tool_name(&self) -> &str { "file_find" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_find".into(),
            description: "Find files by name (case-insensitive substring match). Use to locate files when you know part of the filename.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "directory": {"type":"string","description":"Directory to search"},
                    "name_pattern": {"type":"string","description":"Substring to match in filenames"},
                    "max_depth": {"type":"integer","description":"Max depth (default 5)"},
                    "max_results": {"type":"integer","description":"Max results (default 200)"}
                },
                "required": ["directory", "name_pattern"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool { true }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>, context: &ToolContext) -> Result<ToolResult> {
        let dir = arguments["directory"].as_str().unwrap_or(".");
        let name_pat = arguments["name_pattern"].as_str().unwrap_or("");
        let max_depth = arguments["max_depth"].as_u64().unwrap_or(5) as usize;
        let max_results = arguments["max_results"].as_u64().unwrap_or(200) as usize;
        if name_pat.is_empty() { return Ok(ToolResult { content: "No name_pattern.".into(), is_error: true, structured_content: None }); }
        let resolved = context.resolve_path(dir);
        if let Some(ref guard) = context.sandbox && let Err(reason) = guard.check_read(&resolved) {
            return Ok(ToolResult { content: format!("Sandbox: {}", reason), is_error: true, structured_content: None });
        }
        context.record_read(resolved.clone());
        let mut results = Vec::new();
        find_walk(&resolved, &name_pat.to_lowercase(), 0, max_depth, max_results, &mut results);
        if results.is_empty() { return Ok(ToolResult { content: format!("No files matching '{}'", name_pat), is_error: false, structured_content: None }); }
        let mut out = format!("Found {} files matching '{}':\n", results.len(), name_pat);
        for (path, size) in &results { out.push_str(&format!("  {}  {}\n", format_size(*size), path)); }
        if results.len() >= max_results { out.push_str(&format!("... truncated at {}\n", max_results)); }
        Ok(ToolResult { content: out, is_error: false, structured_content: None })
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

fn find_walk(dir: &Path, pat: &str, depth: usize, max_depth: usize, max_results: usize, results: &mut Vec<(String, u64)>) {
    if depth >= max_depth || results.len() >= max_results { return; }
    if !dir.is_dir() { return; }
    let iter = match std::fs::read_dir(dir) { Ok(i) => i, Err(_) => return };
    for entry in iter.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "node_modules" || name == "target" { continue; }
        let meta = match entry.metadata() { Ok(m) => m, Err(_) => continue };
        if meta.is_dir() { find_walk(&path, pat, depth + 1, max_depth, max_results, results); }
        else if name.to_lowercase().contains(pat) {
            let rel = path.strip_prefix(std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())).unwrap_or(&path).display().to_string();
            results.push((rel, meta.len()));
        }
    }
}

// ============================================================================
// SystemInfo — report session configuration
// ============================================================================

/// Reports the agent's real runtime configuration: active model + provider,
/// sandbox/permission state, workspace, data directory, and OS. Holds clones of
/// the orchestrator's shared config so the values reflect the live session.
pub struct SystemInfoTool {
    pub active_model_name: std::sync::Arc<tokio::sync::RwLock<Option<String>>>,
    pub model_configs:
        std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, loom_types::ModelConfig>>>,
    pub sandbox_config: std::sync::Arc<tokio::sync::RwLock<loom_types::config::SandboxConfig>>,
    pub data_dir: std::path::PathBuf,
}

#[async_trait]
impl AgentTool for SystemInfoTool {
    fn tool_name(&self) -> &str { "system_info" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "system_info".into(),
            description: "Query your own configuration: model, permissions, workspace, skills, MCP servers. Use to check capabilities before acting.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type":"string","description":"What to query: model, permissions, workspace, skills, mcp, or all"}
                },
                "required": ["query"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool { true }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>, context: &ToolContext) -> Result<ToolResult> {
        let query = arguments["query"].as_str().unwrap_or("all");

        // Resolve the active model line from live config.
        let model_line = {
            let active = self.active_model_name.read().await.clone();
            match active {
                Some(name) => {
                    let configs = self.model_configs.read().await;
                    match configs.get(&name) {
                        Some(cfg) => {
                            let provider = cfg.backend.name();
                            let underlying = cfg.model.as_deref().unwrap_or(name.as_str());
                            format!(
                                "Model: {name} (provider: {provider}, model id: {underlying}, context window: {} tokens)",
                                cfg.context_size
                            )
                        }
                        None => format!("Model: {name} (provider: unknown — no matching config)"),
                    }
                }
                None => "Model: none active (configure a model first)".to_string(),
            }
        };

        // Resolve the permission/sandbox line from live sandbox config.
        let perm_line = {
            let sb = self.sandbox_config.read().await;
            if sb.enabled {
                let scope = if sb.workspace_only { "workspace-only" } else { "open" };
                format!(
                    "Permissions: sandbox ENABLED ({scope}); {} extra allowed path(s), {} denied path(s); .loom data access {}",
                    sb.allowed_paths.len(),
                    sb.denied_paths.len(),
                    if sb.allow_loom_data { "permitted" } else { "blocked" }
                )
            } else {
                "Permissions: sandbox DISABLED (filesystem and shell access unrestricted)".to_string()
            }
        };

        let workspace_line = match context.workspace_path {
            Some(ref ws) => format!("Workspace: {ws}"),
            None => "Workspace: not set".to_string(),
        };
        let data_dir_line = format!("Data dir: {}", self.data_dir.display());
        let os_line = format!("OS: {} ({})", std::env::consts::OS, std::env::consts::ARCH);

        let mut info = String::new();
        match query {
            "all" => {
                info.push_str(&workspace_line);
                info.push('\n');
                info.push_str(&data_dir_line);
                info.push('\n');
                info.push_str(&os_line);
                info.push('\n');
                info.push_str(&model_line);
                info.push('\n');
                info.push_str(&perm_line);
                info.push('\n');
                info.push_str("Skills: use 'use_skill' tool to list available skills\n");
                info.push_str("MCP: use 'mcp_list' tool to list connected MCP servers\n");
            }
            "workspace" => {
                info.push_str(&workspace_line);
                info.push('\n');
                info.push_str(&data_dir_line);
                info.push('\n');
                info.push_str(&os_line);
                info.push('\n');
            }
            "model" => { info.push_str(&model_line); info.push('\n'); }
            "permissions" => { info.push_str(&perm_line); info.push('\n'); }
            "skills" => { info.push_str("Skills: use 'use_skill' tool to list available skills\n"); }
            "mcp" => { info.push_str("MCP: use 'mcp_list' tool to list connected MCP servers\n"); }
            _ => { info = format!("Unknown query '{query}'. Valid: model, permissions, workspace, skills, mcp, all"); }
        }
        Ok(ToolResult { content: info, is_error: false, structured_content: None })
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// TokenUsage — check token budget
// ============================================================================

/// Reports real token-usage statistics. Holds a clone of the orchestrator's
/// memory store, which owns the `token_usage` table, and aggregates totals via
/// `get_token_summary`. When no store is wired, it honestly reports that usage
/// tracking is unavailable in this build.
pub struct TokenUsageTool {
    pub memory_store: std::sync::Arc<tokio::sync::RwLock<Option<Box<dyn crate::MemoryStore>>>>,
}

#[async_trait]
impl AgentTool for TokenUsageTool {
    fn tool_name(&self) -> &str { "token_usage" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "token_usage".into(),
            description: "Check your remaining context window budget. Use to pace yourself — avoid abrupt cutoffs on long tasks.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{},"required":[]}),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool { true }

    async fn execute(&self, _arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>, _context: &ToolContext) -> Result<ToolResult> {
        let guard = self.memory_store.read().await;
        let Some(store) = guard.as_ref() else {
            return Ok(ToolResult {
                content: "Token usage tracking is not available in this build (no stats store configured). Monitor your context window and iteration count manually.".into(),
                is_error: false,
                structured_content: None,
            });
        };

        // Aggregate over an all-time window. created_at is an ISO datetime string,
        // so these bounds capture every recorded turn.
        let summary = match store.get_token_summary("0000-01-01", "9999-12-31").await {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolResult {
                    content: format!("Failed to read token usage: {e}"),
                    is_error: true,
                    structured_content: None,
                });
            }
        };

        let prompt = summary["total_prompt_tokens"].as_i64().unwrap_or(0);
        let completion = summary["total_completion_tokens"].as_i64().unwrap_or(0);
        let cached = summary["total_cached_tokens"].as_i64().unwrap_or(0);
        let requests = summary["total_requests"].as_i64().unwrap_or(0);
        let cache_hit = summary["cache_hit_rate"].as_f64().unwrap_or(0.0);
        let avg_latency = summary["avg_latency_ms"].as_f64().unwrap_or(0.0);
        let total = prompt + completion;

        if requests == 0 {
            return Ok(ToolResult {
                content: "No token usage recorded yet for this installation.".into(),
                is_error: false,
                structured_content: Some(summary.clone()),
            });
        }

        let mut out = format!(
            "Token usage (all-time):\n- Total: {total} tokens ({prompt} prompt + {completion} completion)\n- Cached: {cached} tokens (cache hit rate {cache_hit:.1}%)\n- Requests: {requests} (avg latency {avg_latency:.0} ms)\n"
        );
        if let Some(by_model) = summary["by_model"].as_array()
            && !by_model.is_empty()
        {
            out.push_str("By model:\n");
            for m in by_model {
                let name = m["model"].as_str().unwrap_or("(unknown)");
                let p = m["prompt"].as_i64().unwrap_or(0);
                let c = m["completion"].as_i64().unwrap_or(0);
                let r = m["requests"].as_i64().unwrap_or(0);
                out.push_str(&format!("- {name}: {} tokens over {r} requests\n", p + c));
            }
        }

        Ok(ToolResult { content: out, is_error: false, structured_content: Some(summary) })
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// MemorySearch — query knowledge graph
// ============================================================================

/// Searches the agent's knowledge graph. Holds a clone of the orchestrator's
/// memory store so the tool can run a real entity/KG search at execution time.
/// When no store is wired (e.g. memory disabled), the tool reports that memory
/// is unavailable rather than a fake "pending" message.
pub struct MemorySearchTool {
    pub memory_store: std::sync::Arc<tokio::sync::RwLock<Option<Box<dyn crate::MemoryStore>>>>,
}

#[async_trait]
impl AgentTool for MemorySearchTool {
    fn tool_name(&self) -> &str { "memory_search" }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_search".into(),
            description: "Search your knowledge graph for stored information about entities, preferences, or past context. Returns what the system remembers about your query.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type":"string","description":"What to search for in the knowledge graph"},
                    "max_results": {"type":"integer","description":"Max results (default 5)"}
                },
                "required": ["query"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool { true }

    async fn execute(&self, arguments: serde_json::Value, _progress: UnboundedSender<ToolProgress>, _context: &ToolContext) -> Result<ToolResult> {
        let query = arguments["query"].as_str().unwrap_or("");
        if query.is_empty() { return Ok(ToolResult { content: "No query provided.".into(), is_error: true, structured_content: None }); }
        let max_results = arguments["max_results"].as_u64().unwrap_or(5).clamp(1, 50) as usize;

        let guard = self.memory_store.read().await;
        let Some(store) = guard.as_ref() else {
            return Ok(ToolResult {
                content: "Memory not available: no knowledge graph store is configured for this session.".into(),
                is_error: false,
                structured_content: None,
            });
        };

        // Cross-session knowledge search over the KG: (name, entity_type, description, confidence).
        let hits = match store.search_knowledge(query, max_results).await {
            Ok(h) => h,
            Err(e) => {
                return Ok(ToolResult {
                    content: format!("Knowledge graph search failed: {e}"),
                    is_error: true,
                    structured_content: None,
                });
            }
        };

        if hits.is_empty() {
            return Ok(ToolResult {
                content: format!("No knowledge-graph entries found for '{query}'."),
                is_error: false,
                structured_content: None,
            });
        }

        // Enrich with relationship context for the matched entities (best-effort).
        let names: Vec<&str> = hits.iter().map(|(name, _, _, _)| name.as_str()).collect();
        let kg_context = store
            .query_kg_context(&names, max_results, "global")
            .await
            .unwrap_or_default();

        let mut out = format!("Knowledge graph results for '{query}':\n");
        for (name, entity_type, description, confidence) in &hits {
            let desc = if description.is_empty() { "(no description)" } else { description.as_str() };
            out.push_str(&format!(
                "- {name} [{entity_type}] (confidence {confidence:.2}): {desc}\n"
            ));
        }
        if !kg_context.trim().is_empty() {
            out.push_str("\nRelated context:\n");
            out.push_str(kg_context.trim());
            out.push('\n');
        }

        let structured = serde_json::json!({
            "query": query,
            "entities": hits.iter().map(|(name, entity_type, description, confidence)| serde_json::json!({
                "name": name,
                "type": entity_type,
                "description": description,
                "confidence": confidence,
            })).collect::<Vec<_>>(),
            "context": kg_context,
        });

        Ok(ToolResult { content: out, is_error: false, structured_content: Some(structured) })
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
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

    fn supports_parallel(&self) -> bool { true }

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

    fn supports_parallel(&self) -> bool { true }

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

// ============================================================================
// ScheduleReminder — AI 自行判断何时调用，无需硬编码正则
// v2: 存储 AI 提示词而非 shell 命令，触发时由 AI 执行
// ============================================================================

use std::sync::Arc;

pub struct ScheduleReminder {
    pub cron: Arc<tokio::sync::RwLock<Option<Arc<loom_cron::CronScheduler>>>>,
}

#[async_trait]
impl AgentTool for ScheduleReminder {
    fn tool_name(&self) -> &str {
        "schedule_reminder"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "schedule_reminder".into(),
            description: concat!(
                "Create a scheduled AI task. ",
                "Use this when the user asks you to remind them about something, ",
                "set an alarm, create a recurring task, or schedule a future action. ",
                "The 'prompt' is a natural language instruction that will be sent to the AI ",
                "when the schedule fires — the AI will execute it with full tool access. ",
                "Accept 'at' (one-time), 'daily' (every day at HH:MM), or 'interval' (every N minutes). ",
                "Cron expression follows 7-field format: sec min hour day month day_of_week year."
            ).into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Short name (max 20 chars)" },
                    "prompt": { "type": "string", "description": "AI instruction to execute when triggered. Natural language describing what the AI should do (e.g. '检查服务器状态并发送报告', '提醒用户提交代码'). This is NOT a shell command." },
                    "cron_expression": { "type": "string", "description": "7-field cron: sec min hour day month day_of_week year. E.g. '0 0 9 * * * *' for daily 9am. For one-shot tasks, calculate the exact future time." },
                    "kind": { "type": "string", "enum": ["at", "daily", "interval"], "description": "Schedule kind" }
                },
                "required": ["name", "cron_expression", "kind"]
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
        let name = arguments["name"].as_str().unwrap_or("Reminder");
        // prompt is the AI instruction (v2); accept "description" as fallback for v1 compatibility
        let prompt = arguments["prompt"]
            .as_str()
            .or_else(|| arguments["description"].as_str())
            .unwrap_or(name);
        let cron_expr = arguments["cron_expression"].as_str().unwrap_or("");
        let kind = arguments["kind"].as_str().unwrap_or("at");

        if cron_expr.is_empty() {
            return Ok(ToolResult {
                content: "cron_expression is required".into(),
                is_error: true,
                structured_content: None,
            });
        }

        let cron = self.cron.read().await;
        let Some(scheduler) = cron.as_ref() else {
            return Ok(ToolResult { content: "Cron scheduler not available".into(), is_error: true, structured_content: None });
        };

        let mode = loom_cron::job::SessionMode::Isolated;
        match scheduler.add_job(name, cron_expr, prompt, mode, 300).await {
            Ok(id) => {
                let label = match kind {
                    "daily" => "每天 AI 执行",
                    "interval" => "定时 AI 执行",
                    _ => "一次性 AI 任务",
                };
                Ok(ToolResult {
                    content: format!("已创建{}「{}」(id: {})", label, name, &id[..8]),
                    is_error: false,
                    structured_content: Some(serde_json::json!({
                        "id": id, "name": name, "cron": cron_expr, "prompt": prompt
                    })),
                })
            }
            Err(e) => Ok(ToolResult {
                content: format!("创建定时任务失败: {}", e),
                is_error: true,
                structured_content: None,
            }),
        }
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}
