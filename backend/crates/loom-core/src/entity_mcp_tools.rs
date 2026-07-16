//! MCP server management tool - connect, disconnect, list, and delete MCP servers.
//!
//! Runtime MCP config is persisted to the memory store DB, which is the
//! single source of truth for `autostart_mcp_servers` at startup. This keeps
//! connect/delete in sync with autostart (previously this tool wrote only to
//! config.json.mcp, diverging from the DB that autostart reads). config.json's
//! mcp section is now a declarative initial layer, seeded into the DB at startup.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use loom_mcp::{McpClient, McpServerConfig};
use loom_types::config::unified::McpServerEntry;
use loom_types::{ToolDefinition, ToolProgress};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;

use crate::tool_context::ToolContext;
use crate::tool_registry::{AgentTool, ToolProvenance, ToolResult};
use crate::MemoryStore;

// ============================================================================
// ManageMcpTool
// ============================================================================

pub struct ManageMcpTool {
    pub mcp_client: Arc<McpClient>,
    /// Shared memory store (DB) — single source of truth for saved MCP
    /// servers and their autostart flag. Replaces the old config.json.mcp
    /// writer so connect/delete stay in sync with `autostart_mcp_servers`.
    pub memory_store: Arc<RwLock<Option<Box<dyn MemoryStore>>>>,
}

#[async_trait]
impl AgentTool for ManageMcpTool {
    fn tool_name(&self) -> &str {
        "manage_mcp"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "manage_mcp".into(),
            description: "Manage MCP servers (MCP tool servers). Use when user wants to connect an MCP server, disconnect one, list all servers, or delete a saved server config.\n\nCommon scenarios:\n- \"connect to a filesystem MCP server\": action=connect, name=filesystem, transport=stdio, command=npx, args=[-y, @modelcontextprotocol/server-filesystem, /path]\n- \"connect to a remote MCP server\": action=my-server, transport=http, url=http://localhost:3000/mcp\n- \"show connected MCP servers\": action=list\n- \"disconnect the slack server\": action=disconnect, name=slack\n- \"remove old MCP config\": action=delete, name=old-server\n\nActions: list | connect | disconnect | delete.".into(),
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
        let result = exec_mcp(action, &arguments, &self.mcp_client, &self.memory_store).await?;
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
    memory_store: &Arc<RwLock<Option<Box<dyn MemoryStore>>>>,
) -> Result<String> {
    match action {
        "list" => {
            let connected = client.server_names().await;
            let saved = {
                let store = memory_store.read().await;
                if let Some(ref s) = *store {
                    s.list_mcp_servers().await.unwrap_or_default()
                } else {
                    Vec::new()
                }
            };
            let saved_configs: Vec<serde_json::Value> = saved
                .iter()
                .map(|(cfg, autostart)| {
                    serde_json::json!({
                        "name": cfg.name,
                        "transport": cfg.transport,
                        "command": cfg.command,
                        "args": cfg.args,
                        "url": cfg.url,
                        "autostart": autostart,
                    })
                })
                .collect();
            let result = serde_json::json!({
                "connected": connected,
                "saved": saved.iter().map(|(c, _)| c.name.clone()).collect::<Vec<_>>(),
                "saved_configs": saved_configs,
            });
            Ok(serde_json::to_string_pretty(&result).unwrap_or_else(|e| e.to_string()))
        }
        "connect" => {
            let config = build_config(args)?;
            let name = config.name.clone();
            client.connect(config.clone()).await?;
            // Persist to the DB (runtime source of truth) so the server is
            // reconnected by autostart on the next engine start.
            let store = memory_store.read().await;
            if let Some(ref s) = *store {
                s.save_mcp_server(&config, true).await?;
            } else {
                tracing::warn!("memory store unavailable; MCP config not persisted");
            }
            Ok(format!("MCP server \"{name}\" connected and saved to DB (autostart=true)."))
        }
        "disconnect" => {
            let name = req_str(args, "name")?;
            client.disconnect(name).await?;
            Ok(format!("MCP server \"{name}\" disconnected. (DB config kept for reconnect.)"))
        }
        "delete" => {
            let name = req_str(args, "name")?;
            // Best-effort disconnect if live
            let _ = client.disconnect(name).await;
            // Remove from the DB (runtime source of truth) so autostart no
            // longer reconnects it on the next start.
            let store = memory_store.read().await;
            if let Some(ref s) = *store {
                s.delete_mcp_server(name).await?;
            } else {
                tracing::warn!("memory store unavailable; cannot delete MCP config");
            }
            Ok(format!("MCP server \"{name}\" deleted from DB."))
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

/// Convert a `config.json` mcp section entry (the declarative initial layer)
/// into a runtime `McpServerConfig`, used at startup to seed the DB from
/// migrated legacy `mcp.json` content.
pub fn mcp_entry_to_config(name: &str, e: &McpServerEntry) -> McpServerConfig {
    let transport = match e.transport.as_str() {
        "streamableHttp" | "sse" | "http" => "http",
        _ => "stdio",
    };
    McpServerConfig {
        name: name.to_string(),
        transport: transport.to_string(),
        command: e.command.clone(),
        args: e.args.clone(),
        url: e.url.clone(),
        headers: e.headers.clone(),
        env: e.env.clone(),
        cwd: None,
        startup_timeout_secs: 120,
        tool_timeout_secs: 60,
        enabled_tools: None,
        disabled_tools: None,
    }
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
    fn test_mcp_entry_to_config() {
        let entry = McpServerEntry {
            transport: "stdio".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "@playwright/mcp".into()],
            url: None,
            headers: HashMap::new(),
            env: HashMap::new(),
        };
        let cfg = mcp_entry_to_config("playwright", &entry);
        assert_eq!(cfg.name, "playwright");
        assert_eq!(cfg.transport, "stdio");
        assert_eq!(cfg.command, "npx");
        assert_eq!(cfg.args, vec!["-y", "@playwright/mcp"]);

        let http_entry = McpServerEntry {
            transport: "streamableHttp".into(),
            command: String::new(),
            args: Vec::new(),
            url: Some("http://localhost:3000/mcp".into()),
            headers: HashMap::new(),
            env: HashMap::new(),
        };
        let cfg = mcp_entry_to_config("remote", &http_entry);
        assert_eq!(cfg.transport, "http");
        assert_eq!(cfg.url.as_deref(), Some("http://localhost:3000/mcp"));
    }
}
