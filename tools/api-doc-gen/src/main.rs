//! API doc generator — scans dispatch/*.rs and generates docs/api.md
//!
//! Usage: cargo run -p api-doc-gen
//! Output: docs/api.md (overwritten)

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn main() {
    let dispatch_dir = Path::new("backend/crates/loom-server/src/dispatch");
    let out_path = Path::new("docs/api.md");

    let mut categories: BTreeMap<String, Vec<MethodInfo>> = BTreeMap::new();
    let mut push_events: Vec<PushEvent> = Vec::new();

    for entry in fs::read_dir(dispatch_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "rs") { continue; }
        let content = fs::read_to_string(&path).unwrap();
        parse_dispatch_file(&content, &mut categories, &mut push_events);
    }

    let mut out = String::new();

    // Header
    let total = categories.values().flatten().count() as u32;
    out.push_str(&format!("# openLoom Backend API\n\n"));
    out.push_str(&format!("> Auto-generated from dispatch code. Methods: {}\n\n", total));

    out.push_str("## Transport\n\n");
    out.push_str("| 方式 | 端点 | 说明 |\n");
    out.push_str("|------|------|------|\n");
    out.push_str("| **WebSocket** | `ws://{host}:{port}/ws` | 主要通道，支持双向推送 |\n");
    out.push_str("| HTTP POST | `http://{host}:{port}/api` | 兼容通道，无推送 |\n\n");
    out.push_str("协议: **JSON-RPC 2.0**\n\n---\n\n");

    // Error codes
    out.push_str("## 错误码\n\n");
    out.push_str("| 码 | 含义 |\n|----|------|\n");
    out.push_str("| `-32700` | 解析错误 |\n");
    out.push_str("| `-32600` | 请求无效 |\n");
    out.push_str("| `-32601` | 方法不存在 |\n");
    out.push_str("| `-32603` | 内部错误 |\n");
    out.push_str("| `-32001` | Agent 不存在 |\n\n---\n\n");

    // Methods by category
    let category_order = [
        "System", "Chat", "Agent", "Session", "Model", "Workspace",
        "Skills", "Plugins", "Marketplace", "Clawhub",
        "MCP", "LSP", "Tools",
        "KG", "Cognitions", "Memory",
        "Stats", "Config",
    ];

    for cat in &category_order {
        if let Some(methods) = categories.remove(*cat) {
            out.push_str(&format!("## {}\n\n", cat_display(cat)));
            for m in &methods {
                out.push_str(&format!("### {}\n\n", m.name));
                if !m.desc.is_empty() {
                    out.push_str(&format!("{}\n\n", m.desc));
                }
                if !m.params.is_empty() {
                    out.push_str("| Param | 类型 | 必需 | 说明 |\n");
                    out.push_str("|-------|------|:---:|------|\n");
                    for p in &m.params {
                        let req = if p.required { "*" } else { "" };
                        out.push_str(&format!("| `{}` | {} | {} | {} |\n",
                            p.name, p.typ, req, p.desc));
                    }
                    out.push('\n');
                }
                if !m.returns.is_empty() {
                    out.push_str(&format!("```json\n// → {}\n```\n\n", m.returns));
                }
            }
            out.push_str("---\n\n");
        }
    }

    // Remaining categories
    for (cat, methods) in &categories {
        if methods.is_empty() { continue; }
        out.push_str(&format!("## {}\n\n", cat_display(cat)));
        for m in methods {
            out.push_str(&format!("### {}\n\n", m.name));
            if !m.params.is_empty() {
                out.push_str("| Param | 类型 | 必需 | 说明 |\n");
                out.push_str("|-------|------|:---:|------|\n");
                for p in &m.params {
                    let req = if p.required { "*" } else { "" };
                    out.push_str(&format!("| `{}` | {} | {} | {} |\n",
                        p.name, p.typ, req, p.desc));
                }
                out.push('\n');
            }
        }
        out.push_str("---\n\n");
    }

    // Push events
    if !push_events.is_empty() {
        out.push_str("## 服务端推送事件 (WebSocket only)\n\n");
        out.push_str("| 事件 | 参数 | 触发时机 |\n");
        out.push_str("|------|------|----------|\n");
        for e in &push_events {
            out.push_str(&format!("| `{}` | `{}` | {} |\n", e.name, e.params, e.desc));
        }
    }

    fs::write(out_path, out).unwrap();
    println!("Generated {} methods to {}", total, out_path.display());
}

fn cat_display(cat: &str) -> &str {
    match cat {
        "KG" => "Knowledge Graph",
        "MCP" => "MCP",
        "LSP" => "LSP",
        "Memory" => "Memory 管线",
        _ => cat,
    }
}

struct MethodInfo {
    name: String,
    desc: String,
    params: Vec<ParamInfo>,
    returns: String,
}

struct ParamInfo {
    name: String,
    typ: String,
    required: bool,
    desc: String,
}

struct PushEvent {
    name: String,
    params: String,
    desc: String,
}

fn category_for(method: &str) -> String {
    let parts: Vec<&str> = method.splitn(2, '.').collect();
    let prefix = parts[0];
    match prefix {
        "system" => "System".into(),
        "chat" => "Chat".into(),
        "agent" => "Agent".into(),
        "session" => "Session".into(),
        "model" => "Model".into(),
        "workspace" => "Workspace".into(),
        "skills" => "Skills".into(),
        "plugins" => "Plugins".into(),
        "marketplace" => "Marketplace".into(),
        "clawhub" => "Clawhub".into(),
        "mcp" => "MCP".into(),
        "lsp" => "LSP".into(),
        "tools" | "tool" => "Tools".into(),
        "kg" | "memory" | "cognitions" => prefix.to_string(),
        "stats" => "Stats".into(),
        "config" => "Config".into(),
        _ => prefix.to_string(),
    }
}

fn parse_dispatch_file(
    content: &str,
    categories: &mut BTreeMap<String, Vec<MethodInfo>>,
    _push_events: &mut Vec<PushEvent>,
) {
    // Extract method names from match arms
    let mut methods: Vec<String> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Match: "method.name" => Some(handler...
        if let Some(start) = trimmed.find('"') {
            let rest = &trimmed[start + 1..];
            if let Some(end) = rest.find('"') {
                let method = &rest[..end];
                if method.contains('.')
                    && rest[end..].contains("=>")
                    && !method.contains(' ')  // exclude file paths etc
                    && method.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
                {
                    methods.push(method.to_string());
                }
            }
        }
    }

    // Extract parameters for each method
    for method in &methods {
        let handler_name = method_to_handler(method);
        let params = extract_params(content, method, &handler_name);
        let returns = extract_return(content, &handler_name);

        let cat = category_for(method);
        categories.entry(cat).or_default().push(MethodInfo {
            name: method.clone(),
            desc: String::new(),
            params,
            returns,
        });
    }
}

fn method_to_handler(method: &str) -> String {
    method.replace('.', "_")
}

fn extract_params(content: &str, _method: &str, handler_name: &str) -> Vec<ParamInfo> {
    let mut params = Vec::new();

    // Find the handler function body — match fn handle_{handler_name}(
    let fn_pattern = format!("fn handle_{handler_name}(");
    let fn_start = match content.find(&fn_pattern) {
        Some(i) => i,
        None => return params,
    };

    // Find function body start (first { after fn_start)
    let body_start = match content[fn_start..].find('{') {
        Some(i) => fn_start + i + 1,
        None => return params,
    };

    // Find function end by tracking brace depth
    let end = find_fn_end(&content[body_start - 1..]).map(|i| body_start - 1 + i).unwrap_or(content.len());
    let body = &content[body_start..end];

    // Find p.get("X") patterns
    let mut seen = std::collections::HashSet::new();
    let mut pos = 0;
    while let Some(idx) = body[pos..].find("p.get(\"") {
        let abs_idx = pos + idx + 7; // after p.get("
        let rest = &body[abs_idx..];
        if let Some(quote_end) = rest.find('"') {
            let param_name = &rest[..quote_end];
            if !seen.contains(param_name) && !param_name.is_empty() {
                seen.insert(param_name.to_string());
                let after = &rest[quote_end + 1..];
                let typ = infer_type(after);
                let (required, _default) = infer_required(after);
                params.push(ParamInfo {
                    name: param_name.to_string(),
                    typ,
                    required,
                    desc: String::new(),
                });
            }
        }
        pos = abs_idx + 1;
        if pos >= body.len() { break; }
    }

    params
}

fn find_fn_end(s: &str) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut depth = 0;
    let mut started = false;
    for (i, &c) in chars.iter().enumerate() {
        if c == '{' {
            started = true;
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if started && depth == 0 {
                return Some(i + 1);
            }
        }
    }
    None
}

fn infer_type(after_quote: &str) -> String {
    let s = after_quote.trim();
    if s.contains("as_str") { return "string".into(); }
    if s.contains("as_bool") { return "bool".into(); }
    if s.contains("as_u64") || s.contains("as_i64") { return "number".into(); }
    if s.contains("as_f64") { return "number".into(); }
    if s.contains("as_array") { return "array".into(); }
    if s.contains("as_object") { return "object".into(); }
    "any".into()
}

fn infer_required(after_param: &str) -> (bool, String) {
    // Look for .unwrap_or() — if present, the param is optional
    let ctx = &after_param[..after_param.len().min(200)];
    if ctx.contains("unwrap_or(") || ctx.contains("unwrap_or_default()") {
        return (false, String::new());
    }
    // Look for ok_or_else with "required" message
    if ctx.contains("required") || ctx.contains("ok_or_else") {
        return (true, String::new());
    }
    (false, String::new())
}

fn extract_return(content: &str, handler_name: &str) -> String {
    let fn_pattern = format!("fn handle_{handler_name}(");
    let fn_start = match content.find(&fn_pattern) {
        Some(i) => i,
        None => return String::new(),
    };

    let body_start = match content[fn_start..].find('{') {
        Some(i) => fn_start + i + 1,
        None => return String::new(),
    };

    let end = find_fn_end(&content[body_start - 1..]).map(|i| body_start - 1 + i).unwrap_or(content.len());
    let body = &content[body_start..end];

    // Find last Ok(json!(...)) — this is typically the success return
    if let Some(ok_idx) = body.rfind("Ok(json!(") {
        let json_start = ok_idx + 9;
        let rest = &body[json_start..];
        if let Some(close) = find_matching_brace(rest) {
            let val = rest[..close].trim().to_string();
            if !val.is_empty() {
                return val;
            }
        }
    }

    String::new()
}

fn find_matching_brace(s: &str) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut depth = 0;
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1); // include the closing }
                }
            }
            _ => {}
        }
    }
    None
}
