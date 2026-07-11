//! MCP server management tool — connect, disconnect, list, and delete MCP servers.
//! Persists configs to mcp.json in data_dir; delegates live operations to McpClient.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use loom_mcp::{McpClient, McpServerConfig};
use loom_types::{ToolDefinition, ToolProgress};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::tool_context::ToolContext;
use crate::tool_registry::{AgentTool, ToolProvenance, ToolResult};

// ============================================================================
// mcp.json file format (subset used by this tool)
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Default)]
struct McpJsonFile {
    #[serde(default)]
    #[serde(rename = "mcpServers")]
    mcp_servers: HashMap<String, McpJsonEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct McpJsonEntry {
    #[serde(rename = "type", default = "default_transport")]
    transport: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

fn default_transport() -> String {
    "stdio".into()
}

// ============================================================================
// ManageMcpTool
// ============================================================================

pub struct ManageMcpTool {
    pub mcp_client: Arc<McpClient>,
    pub data_dir: PathBuf,
}

#[async_trait]
impl AgentTool for ManageMcpTool {
    fn tool_name(&self) -> &str {
        "manage_mcp"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "manage_mcp".into(),
            description: "Manage MCP (Model Context Protocol) servers. Use when user says connect/add/remove/disconnect an MCP server or wants to see which MCP servers are available.\n\nActions: list (list connected + saved servers), connect (connect and register a server), disconnect (disconnect a live server), delete (disconnect + remove saved config). Required: action. For connect: name + transport + (command if stdio, url if http). For disconnect/delete: name.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "connect", "disconnect", "delete"],
                        "description": "Action to perform"
                    },
                    "name": {
                        "type": "string",
                        "description": "Server name (required for connect/disconnect/delete)"
                    },
                    "transport": {
                        "type": "string",
                        "enum": ["stdio", "http"],
                        "description": "Transport type: 'stdio' for command-line servers, 'http' for HTTP/SSE servers"
                    },
                    "command": {
                        "type": "string",
                        "description": "Executable command for stdio transport (e.g. 'npx', 'python', 'node')"
                    },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Command-line arguments (for stdio transport)"
                    },
                    "url": {
                        "type": "string",
                        "description": "HTTP URL for the MCP server endpoint (for http transport)"
                    },
                    "headers": {
                        "type": "object",
                        "description": "HTTP headers as key-value pairs (for http transport)"
                    },
                    "env": {
                        "type": "object",
                        "description": "Environment variables for the server process (for stdio transport)"
                    }
                },
                "required": ["action"]
            }),
            tags: vec!["mcp".into(), "configuration".into()],
        }
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let action = arguments["action"].as_str().unwrap_or("");
        let result = exec_mcp(action, &arguments, &self.mcp_client, &self.data_dir).await?;
        Ok(ToolResult {
            content: result,
            is_error: false,
            structured_content: None,
        })
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

// ============================================================================
// Action dispatch
// ============================================================================

async fn exec_mcp(
    action: &str,
    args: &serde_json::Value,
    client: &McpClient,
    data_dir: &PathBuf,
) -> Result<String> {
    match action {
        "list" => {
            let connected = client.server_names().await;
            let saved = load_saved_configs(data_dir);
            let result = serde_json::json!({
                "connected": connected,
                "saved": saved.keys().collect::<Vec<&String>>(),
                "saved_configs": saved,
            });
            Ok(serde_json::to_string_pretty(&result).unwrap_or_else(|e| e.to_string()))
        }
        "connect" => {
            let config = build_config(args)?;
            let name = config.name.clone();
            client.connect(config).await?;
            // Persist to mcp.json
            save_config_to_json(data_dir, args, &name)?;
            Ok(format!("MCP server \"{name}\" connected."))
        }
        "disconnect" => {
            let name = req_str(args, "name")?;
            client.disconnect(name).await?;
            Ok(format!("MCP server \"{name}\" disconnected."))
        }
        "delete" => {
            let name = req_str(args, "name")?;
            // Best-effort disconnect if live
            let _ = client.disconnect(name).await;
            // Remove from mcp.json
            remove_config_from_json(data_dir, name)?;
            Ok(format!("MCP server \"{name}\" deleted."))
        }
        _ => Err(anyhow::anyhow!(
            "Unknown action: {action}. Use list | connect | disconnect | delete."
        )),
    }
}

// ============================================================================
// Config builders
// ============================================================================

fn build_config(args: &serde_json::Value) -> Result<McpServerConfig> {
    let name = req_str(args, "name")?;
    let transport = args["transport"].as_str().unwrap_or("stdio");

    let mut config = McpServerConfig {
        name: name.to_string(),
        transport: transport.to_string(),
        command: String::new(),
        args: Vec::new(),
        url: None,
        headers: HashMap::new(),
        env: HashMap::new(),
        cwd: None,
        startup_timeout_secs: 120,
        tool_timeout_secs: 60,
        enabled_tools: None,
        disabled_tools: None,
    };

    match transport {
        "http" => {
            let url = args["url"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("url required for http transport"))?;
            config.url = Some(url.to_string());
            if let Some(headers) = args["headers"].as_object() {
                for (k, v) in headers {
                    if let Some(val) = v.as_str() {
                        config.headers.insert(k.clone(), val.to_string());
                    }
                }
            }
        }
        _ => {
            // stdio
            let command = args["command"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("command required for stdio transport"))?;
            config.command = command.to_string();
            if let Some(arr) = args["args"].as_array() {
                config.args = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
            }
            if let Some(env) = args["env"].as_object() {
                for (k, v) in env {
                    if let Some(val) = v.as_str() {
                        config.env.insert(k.clone(), val.to_string());
                    }
                }
            }
        }
    }

    Ok(config)
}

// ============================================================================
// mcp.json persistence helpers
// ============================================================================

fn mcp_json_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("mcp.json")
}

fn load_mcp_json_file(data_dir: &PathBuf) -> McpJsonFile {
    let path = mcp_json_path(data_dir);
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => McpJsonFile::default(),
        }
    } else {
        McpJsonFile::default()
    }
}

fn save_mcp_json_file(data_dir: &PathBuf, file: &McpJsonFile) -> Result<()> {
    let path = mcp_json_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(file)?;
    std::fs::write(&path, json)?;
    Ok(())
}

fn load_saved_configs(data_dir: &PathBuf) -> HashMap<String, McpJsonEntry> {
    load_mcp_json_file(data_dir).mcp_servers
}

fn save_config_to_json(data_dir: &PathBuf, args: &serde_json::Value, name: &str) -> Result<()> {
    let mut file = load_mcp_json_file(data_dir);
    let transport = args["transport"].as_str().unwrap_or("stdio");

    let entry = McpJsonEntry {
        transport: transport.to_string(),
        command: args["command"].as_str().unwrap_or("").to_string(),
        args: args["args"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        url: args["url"].as_str().map(|s| s.to_string()),
        headers: args["headers"]
            .as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
        env: args["env"]
            .as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
    };

    file.mcp_servers.insert(name.to_string(), entry);
    save_mcp_json_file(data_dir, &file)?;
    tracing::info!(server = %name, "saved MCP config to mcp.json");
    Ok(())
}

fn remove_config_from_json(data_dir: &PathBuf, name: &str) -> Result<()> {
    let mut file = load_mcp_json_file(data_dir);
    if file.mcp_servers.remove(name).is_some() {
        save_mcp_json_file(data_dir, &file)?;
        tracing::info!(server = %name, "removed MCP config from mcp.json");
    }
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn req_str<'a>(args: &'a serde_json::Value, field: &str) -> Result<&'a str> {
    args[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("{field} required"))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a temporary directory that is cleaned up on drop.
    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("loom-test-mcp-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            TestDir { path }
        }

        fn data_dir(&self) -> PathBuf {
            self.path.clone()
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_build_config_stdio() {
        let args = serde_json::json!({
            "name": "test-server",
            "transport": "stdio",
            "command": "npx",
            "args": ["-y", "@test/mcp"]
        });
        let config = build_config(&args).unwrap();
        assert_eq!(config.name, "test-server");
        assert_eq!(config.transport, "stdio");
        assert_eq!(config.command, "npx");
        assert_eq!(config.args, vec!["-y", "@test/mcp"]);
        assert!(config.url.is_none());
    }

    #[test]
    fn test_build_config_http() {
        let args = serde_json::json!({
            "name": "http-server",
            "transport": "http",
            "url": "http://localhost:8080/mcp",
            "headers": {
                "Authorization": "Bearer token123",
                "X-Custom": "value"
            }
        });
        let config = build_config(&args).unwrap();
        assert_eq!(config.name, "http-server");
        assert_eq!(config.transport, "http");
        assert_eq!(config.url.as_deref(), Some("http://localhost:8080/mcp"));
        assert_eq!(config.headers.get("Authorization").map(|v| v.as_str()), Some("Bearer token123"));
        assert_eq!(config.headers.get("X-Custom").map(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_build_config_missing_name() {
        let args = serde_json::json!({
            "transport": "stdio",
            "command": "npx"
        });
        assert!(build_config(&args).is_err());
    }

    #[test]
    fn test_build_config_stdio_missing_command() {
        let args = serde_json::json!({
            "name": "no-cmd",
            "transport": "stdio"
        });
        assert!(build_config(&args).is_err());
    }

    #[test]
    fn test_build_config_http_missing_url() {
        let args = serde_json::json!({
            "name": "no-url",
            "transport": "http"
        });
        assert!(build_config(&args).is_err());
    }

    #[test]
    fn test_build_config_defaults() {
        let args = serde_json::json!({
            "name": "minimal",
            "command": "echo"
        });
        let config = build_config(&args).unwrap();
        assert_eq!(config.name, "minimal");
        assert_eq!(config.transport, "stdio");
        assert_eq!(config.command, "echo");
        assert!(config.args.is_empty());
        assert_eq!(config.startup_timeout_secs, 120);
        assert_eq!(config.tool_timeout_secs, 60);
        assert!(config.enabled_tools.is_none());
        assert!(config.disabled_tools.is_none());
    }

    #[test]
    fn test_req_str() {
        let args = serde_json::json!({"name": "test", "empty": ""});
        assert_eq!(req_str(&args, "name").unwrap(), "test");
        assert!(req_str(&args, "empty").is_err());
        assert!(req_str(&args, "missing").is_err());
    }

    #[test]
    fn test_mcp_json_roundtrip() {
        let dir = TestDir::new();
        let data_dir = dir.data_dir();

        let args = serde_json::json!({
            "name": "playwright",
            "transport": "stdio",
            "command": "npx",
            "args": ["-y", "@playwright/mcp"],
            "env": {"NODE_ENV": "test"}
        });
        save_config_to_json(&data_dir, &args, "playwright").unwrap();

        let saved = load_saved_configs(&data_dir);
        assert!(saved.contains_key("playwright"));
        let entry = saved.get("playwright").unwrap();
        assert_eq!(entry.transport, "stdio");
        assert_eq!(entry.command, "npx");
        assert_eq!(entry.args, vec!["-y", "@playwright/mcp"]);

        remove_config_from_json(&data_dir, "playwright").unwrap();
        let after = load_saved_configs(&data_dir);
        assert!(!after.contains_key("playwright"));
    }

    #[test]
    fn test_remove_nonexistent_ok() {
        let dir = TestDir::new();
        let data_dir = dir.data_dir();
        // Should not error when removing a server that doesn't exist
        assert!(remove_config_from_json(&data_dir, "nonexistent").is_ok());
    }
}
