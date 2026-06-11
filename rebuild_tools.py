#!/usr/bin/env python3
"""Rebuild builtin_tools.rs with all changes applied."""
import re

PATH = 'backend/crates/loom-core/src/builtin_tools.rs'
content = open(PATH, 'r', encoding='utf-8').read()

# ── 1. supports_parallel() on 5 read-only tools ──
for (struct_name, tool_name) in [('FileReadTool', 'file_read'), ('FileListTool', 'file_list'),
    ('ContentSearchTool', 'content_search'), ('WebSearchTool', 'web_search'), ('WebFetchTool', 'web_fetch')]:
    old = f'impl AgentTool for {struct_name} {{\n    fn tool_name(&self) -> &str {{\n        "{tool_name}"\n    }}'
    new = f'impl AgentTool for {struct_name} {{\n    fn tool_name(&self) -> &str {{\n        "{tool_name}"\n    }}\n\n    fn supports_parallel(&self) -> bool {{ true }}'
    if old in content:
        content = content.replace(old, new)
        print(f'  + supports_parallel on {struct_name}')
    else:
        print(f'  WARN: pattern not found for {struct_name}')

# ── 2. FileEditTool: insert after FileWriteTool ──
file_edit = '''
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
        let normalized = old_content.replace("\\r\\n", "\\n");
        let edits_raw: Vec<(&str, &str)> = if let Some(arr) = arguments["edits"].as_array() {
            if arr.is_empty() { return Ok(ToolResult { content: "Empty edits array.".into(), is_error: true, structured_content: None }); }
            arr.iter().map(|e| (e["oldText"].as_str().unwrap_or(""), e["newText"].as_str().unwrap_or(""))).collect()
        } else {
            let ot = arguments["oldText"].as_str().unwrap_or("");
            if ot.is_empty() { return Ok(ToolResult { content: "No oldText provided.".into(), is_error: true, structured_content: None }); }
            vec![(ot, arguments["newText"].as_str().unwrap_or(""))]
        };
        let edits: Vec<(String, String)> = edits_raw.into_iter().map(|(o,n)| (o.replace("\\r\\n", "\\n"), n.replace("\\r\\n", "\\n"))).collect();
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
        if old_content.contains("\\r\\n") { result = result.replace("\\n", "\\r\\n"); }
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        match std::fs::write(&path, &result) {
            Ok(_) => Ok(ToolResult { content: format!("Edited: {} ({} changes)", path_str, eps.len()), is_error: false, structured_content: Some(serde_json::json!({"filePath":path_str,"fileName":file_name,"oldContent":old_content,"newContent":result})) }),
            Err(e) => Ok(ToolResult { content: format!("Write failed: {}", e), is_error: true, structured_content: None }),
        }
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}
'''
marker = '\n// ============================================================================\n// ContentSearch (grep equivalent)\n// ============================================================================'
content = content.replace(marker, file_edit + marker)
print('  + FileEditTool inserted')

# ── 3. Read-Before-Edit guard in FileWriteTool ──
old = '''        // Sandbox guard: check write permission
        if let Some(ref guard) = context.sandbox
            && let Err(reason) = guard.check_write(&path)
        {
            return Ok(ToolResult {
                content: format!("沙盒拒绝: {}", reason),
                is_error: true,
                structured_content: None,
            });
        }
        let old_content = std::fs::read_to_string(&path).unwrap_or_default();'''
new = '''        // Sandbox guard: check write permission
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
        let old_content = std::fs::read_to_string(&path).unwrap_or_default();'''
if old in content:
    content = content.replace(old, new)
    print('  + Read-Before-Edit guard on FileWriteTool')
else:
    print('  WARN: FileWriteTool guard pattern not found')

# ── 4. Fix FileReadTool record_read call order ──
old = '''        context.record_read(path.clone());
        let content = std::fs::read_to_string(&path)?;'''
new = '''        let content = std::fs::read_to_string(&path)?;
        context.record_read(path.clone());'''
if old in content:
    content = content.replace(old, new)
    print('  + Fixed record_read order in FileReadTool')

# ── 5. Insert AskUserTool + GlobTool + FindTool + SystemInfoTool + TokenUsageTool + MemorySearchTool ──
# Insert all before WebSearchTool
new_tools = '''
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
            content.push_str("\\n\\nOptions:");
            for (i, o) in options.iter().enumerate() { content.push_str(&format!("\\n  {}. {}", i+1, o)); }
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
        let mut out = format!("Found {} files matching '{}':\\n", results.len(), pattern);
        for (path, size) in &results { out.push_str(&format!("  {}  {}\\n", format_size(*size), path)); }
        if results.len() >= max_results { out.push_str(&format!("... truncated at {}\\n", max_results)); }
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
        let mut out = format!("Found {} files matching '{}':\\n", results.len(), name_pat);
        for (path, size) in &results { out.push_str(&format!("  {}  {}\\n", format_size(*size), path)); }
        if results.len() >= max_results { out.push_str(&format!("... truncated at {}\\n", max_results)); }
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

pub struct SystemInfoTool;

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
        let mut info = String::new();
        match query {
            "workspace" | "all" => {
                if let Some(ref ws) = context.workspace_path { info.push_str(&format!("Workspace: {}\\n", ws)); }
                else { info.push_str("Workspace: not set\\n"); }
            }
            "model" | "all" => { info.push_str("Model: (check configuration)\\n"); }
            "permissions" | "all" => { info.push_str("Permissions: (check sandbox/security config)\\n"); }
            "skills" | "all" => { info.push_str("Skills: use 'use_skill' tool to list available skills\\n"); }
            "mcp" | "all" => { info.push_str("MCP: (check mcp_list)\\n"); }
            _ => { info = format!("Unknown query '{}'. Valid: model, permissions, workspace, skills, mcp, all", query); }
        }
        Ok(ToolResult { content: info, is_error: false, structured_content: None })
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// TokenUsage — check token budget
// ============================================================================

pub struct TokenUsageTool;

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
        Ok(ToolResult { content: "Token tracking is internal. Monitor your context window size and iteration count. If your responses get truncated or you approach the max_iterations limit, summarize progress and ask the user to continue.".into(), is_error: false, structured_content: None })
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}

// ============================================================================
// MemorySearch — query knowledge graph
// ============================================================================

pub struct MemorySearchTool;

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
        Ok(ToolResult { content: format!("Knowledge graph search for '{}': KG integration is pending. The system automatically injects relevant context into each turn. For now, rely on the auto-injected context and conversation history for recall.", query), is_error: false, structured_content: None })
    }
    fn provenance(&self) -> ToolProvenance { ToolProvenance::Builtin }
}
'''
old_marker = '\n// ============================================================================\n// WebSearch — DuckDuckGo Lite search (no API key needed)\n// ============================================================================'
content = content.replace(old_marker, new_tools + old_marker)
print('  + Inserted AskUserTool, GlobTool, FindTool, SystemInfoTool, TokenUsageTool, MemorySearchTool')

# Write result
open(PATH, 'w', encoding='utf-8').write(content)
print(f'Done. {len(content)} chars written.')
