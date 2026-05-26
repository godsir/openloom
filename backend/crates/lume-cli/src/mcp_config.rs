//! MCP configuration loader — reads Claude Code-compatible .mcp.json files.
//!
//! Format:
//! ```json
//! {
//!   "mcpServers": {
//!     "server_name": {
//!       "type": "stdio" | "streamableHttp",
//!       "command": "...",
//!       "args": ["..."],
//!       "url": "http://...",
//!       "headers": { "X-Token": "..." }
//!     }
//!   }
//! }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Root of .mcp.json config file.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct McpConfigFile {
    #[serde(default)]
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpConfigEntry>,
}

/// A single MCP server entry in the config file.
#[derive(Debug, Serialize, Deserialize)]
pub struct McpConfigEntry {
    #[serde(rename = "type", default = "default_mcp_type")]
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

fn default_mcp_type() -> String { "stdio".into() }

/// Load MCP servers from config files.
///
/// Scans in priority order:
/// 1. Project-level: `<cwd>/.lume/mcp.json`
/// 2. User-level: `<data_dir>/mcp.json`
pub fn load_mcp_configs(data_dir: &PathBuf) -> Result<(Vec<lume_mcp::McpServerConfig>, Vec<String>)> {
    let mut configs = Vec::new();
    let mut sources = Vec::new();

    // Project config
    if let Ok(cwd) = std::env::current_dir() {
        let project_path = cwd.join(".lume").join("mcp.json");
        if project_path.exists() {
            match load_config_file(&project_path) {
                Ok(mut cfgs) => {
                    configs.append(&mut cfgs);
                    sources.push(format!("project: {}", project_path.display()));
                }
                Err(e) => tracing::warn!(path=%project_path.display(), error=%e, "failed to load project MCP config"),
            }
        }
    }

    // User config
    let user_path = data_dir.join("mcp.json");
    if user_path.exists() {
        match load_config_file(&user_path) {
            Ok(mut cfgs) => {
                configs.append(&mut cfgs);
                sources.push(format!("user: {}", user_path.display()));
            }
            Err(e) => tracing::warn!(path=%user_path.display(), error=%e, "failed to load user MCP config"),
        }
    }

    Ok((configs, sources))
}

fn load_config_file(path: &std::path::Path) -> Result<Vec<lume_mcp::McpServerConfig>> {
    let content = std::fs::read_to_string(path)?;
    let parsed: McpConfigFile = serde_json::from_str(&content)?;
    let mut configs = Vec::new();

    for (name, entry) in &parsed.mcp_servers {
        let transport = match entry.transport.as_str() {
            "streamableHttp" | "sse" | "http" => "http",
            _ => "stdio",
        };
        configs.push(lume_mcp::McpServerConfig {
            name: name.clone(),
            transport: transport.to_string(),
            url: entry.url.clone(),
            headers: entry.headers.clone(),
            command: entry.command.clone(),
            args: entry.args.clone(),
            env: Default::default(),
            cwd: None,
            startup_timeout_secs: 30,
            tool_timeout_secs: 60,
            enabled_tools: None,
            disabled_tools: None,
        });
    }

    tracing::info!(path=%path.display(), count=configs.len(), "loaded MCP configs");
    Ok(configs)
}

/// Merge CLI mcp-args into the configs list.
pub fn parse_mcp_args(args: &str) -> Result<lume_mcp::McpServerConfig> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() { anyhow::bail!("empty mcp-args"); }

    let first = parts[0];
    if first.starts_with("http://") || first.starts_with("https://") {
        let url = first.to_string();
        let name = url.rsplit('/').next().unwrap_or("mcp").to_string();
        let mut headers = HashMap::new();
        for part in &parts[1..] {
            if let Some((k, v)) = part.split_once('=') {
                headers.insert(k.to_string(), v.to_string());
            }
        }
        Ok(lume_mcp::McpServerConfig {
            name, transport: "http".into(), url: Some(url), headers,
            command: String::new(), args: vec![], env: Default::default(), cwd: None,
            startup_timeout_secs: 30, tool_timeout_secs: 60,
            enabled_tools: None, disabled_tools: None,
        })
    } else if parts.len() >= 2 {
        let name = parts.last().map(|s| s.rsplit(&['\\', '/']).next().unwrap_or(s)).unwrap_or("mcp");
        Ok(lume_mcp::McpServerConfig {
            name: name.to_string(), transport: "stdio".into(), command: parts[0].into(),
            args: parts[1..].iter().map(|s| s.to_string()).collect(),
            url: None, headers: Default::default(), env: Default::default(), cwd: None,
            startup_timeout_secs: 30, tool_timeout_secs: 60,
            enabled_tools: None, disabled_tools: None,
        })
    } else {
        anyhow::bail!("usage: 'http://url header=val' or 'command args...'")
    }
}
