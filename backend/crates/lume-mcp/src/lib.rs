// SPDX-License-Identifier: Apache-2.0
//! MCP (Model Context Protocol) client for openLoom v2.
//!
//! Supports both stdio transport (spawning MCP server processes) and
//! HTTP transport (POST JSON-RPC 2.0 to an MCP endpoint).

use anyhow::{anyhow, Context, Result};
use loom_types::{McpResource, McpResourceContent, McpTool, ToolDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

// ============================================================================
// Config
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout_secs: u64,
    #[serde(default = "default_tool_timeout")]
    pub tool_timeout_secs: u64,
    #[serde(default)]
    pub enabled_tools: Option<Vec<String>>,
    #[serde(default)]
    pub disabled_tools: Option<Vec<String>>,
}

fn default_transport() -> String { "stdio".into() }
fn default_startup_timeout() -> u64 { 30 }
fn default_tool_timeout() -> u64 { 60 }

// ============================================================================
// Stdio connection
// ============================================================================

struct McpConnection {
    process: Child,
    tools: Vec<McpTool>,
    next_id: u64,
}

impl McpConnection {
    async fn handshake(config: McpServerConfig) -> Result<Self> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);
        for (k, v) in &config.env { cmd.env(k, v); }
        if let Some(ref cwd) = config.cwd { cmd.current_dir(cwd); }

        let mut process = cmd.spawn()
            .with_context(|| format!("failed to spawn '{}'. Is it installed?", config.command))?;

        let stdin = process.stdin.take().ok_or_else(|| anyhow!("stdin unavailable"))?;
        let stdout = process.stdout.take().ok_or_else(|| anyhow!("stdout unavailable"))?;
        let mut writer = tokio::io::BufWriter::new(stdin);
        let mut reader = BufReader::new(stdout);
        let mut conn = Self { process, tools: Vec::new(), next_id: 1 };

        // init
        let result = conn.send_request(&mut writer, &mut reader, "initialize", &serde_json::json!({
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "openLoom", "version": "0.2.0"}
        })).await?;
        tracing::info!(server=%config.name, version=%result["protocolVersion"], "MCP init");

        // initialized notification
        conn.send_notification(&mut writer, "notifications/initialized", &serde_json::json!({})).await?;

        // tools/list
        let tools_resp = conn.send_request(&mut writer, &mut reader, "tools/list", &serde_json::json!({})).await?;
        conn.tools = parse_tool_list(&tools_resp, &config);
        tracing::info!(server=%config.name, count=conn.tools.len(), "MCP tools");

        Ok(conn)
    }

    async fn send_request(&mut self, w: &mut (impl AsyncWriteExt + Unpin), r: &mut (impl AsyncBufReadExt + Unpin), method: &str, params: &Value) -> Result<Value> {
        let id = self.next_id; self.next_id += 1;
        let mut body = serde_json::to_string(&serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        body.push('\n');
        w.write_all(body.as_bytes()).await?; w.flush().await?;

        let mut line = String::new();
        r.read_line(&mut line).await?;
        let resp: Value = serde_json::from_str(&line)
            .with_context(|| format!("MCP parse error: {}", truncate(&line, 200)))?;
        if let Some(err) = resp.get("error") {
            return Err(anyhow!("MCP error: {}", err["message"].as_str().unwrap_or("unknown")));
        }
        Ok(resp["result"].clone())
    }

    async fn send_notification(&self, w: &mut (impl AsyncWriteExt + Unpin), method: &str, params: &Value) -> Result<()> {
        let mut body = serde_json::to_string(&serde_json::json!({"jsonrpc":"2.0","method":method,"params":params}))?;
        body.push('\n');
        w.write_all(body.as_bytes()).await?; w.flush().await?;
        Ok(())
    }
}

fn parse_tool_list(result: &Value, config: &McpServerConfig) -> Vec<McpTool> {
    let tools = result["tools"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
    tools.iter().filter_map(|t| {
        let name = t["name"].as_str()?.to_string();
        if let Some(ref a) = config.enabled_tools { if !a.contains(&name) { return None; } }
        if let Some(ref d) = config.disabled_tools { if d.contains(&name) { return None; } }
        Some(McpTool {
            name, title: t["title"].as_str().map(String::from),
            description: t["description"].as_str().unwrap_or("").to_string(),
            input_schema: t["inputSchema"].clone(),
            output_schema: None, annotations: None, icons: None, meta: None,
        })
    }).collect()
}

// ============================================================================
// Connection types
// ============================================================================

enum ServerConn {
    Stdio { conn: McpConnection, stdin: tokio::io::BufWriter<tokio::process::ChildStdin>, stdout: BufReader<tokio::process::ChildStdout> },
    Http { tools: Vec<McpTool>, url: String, headers: reqwest::header::HeaderMap, client: reqwest::Client, next_id: AtomicU64 },
}

impl ServerConn {
    fn tools(&self) -> &[McpTool] {
        match self { ServerConn::Stdio { conn, .. } => &conn.tools, ServerConn::Http { tools, .. } => tools }
    }
}

// ============================================================================
// McpClient
// ============================================================================

pub struct McpClient {
    servers: Arc<RwLock<HashMap<String, ServerConn>>>,
    tool_prefix: String,
}

impl McpClient {
    pub fn new() -> Self { Self { servers: Arc::new(RwLock::new(HashMap::new())), tool_prefix: "mcp".into() } }

    pub async fn connect(&self, config: McpServerConfig) -> Result<String> {
        let name = config.name.clone();
        let conn = if config.transport == "http" {
            let url = config.url.as_ref().ok_or_else(|| anyhow!("HTTP transport needs 'url'"))?.trim_end_matches('/').to_string();
            let mut headers = reqwest::header::HeaderMap::new();
            for (k, v) in &config.headers {
                headers.insert(
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()).map_err(|e| anyhow!("bad header '{}': {}", k, e))?,
                    reqwest::header::HeaderValue::from_str(v).map_err(|e| anyhow!("bad value '{}': {}", k, e))?,
                );
            }
            // MCP HTTP spec requires Accept: application/json, text/event-stream
            headers.insert(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/json, text/event-stream"),
            );
            let client = reqwest::Client::new();

                let r = mcp_http_post(&client, &url, &headers, &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"openLoom","version":"0.2.0"}}})).await?;
            if r.get("error").is_some() { anyhow::bail!("MCP init: {}", r["error"]["message"].as_str().unwrap_or("?")); }

            mcp_http_notify(&client, &url, &headers, &serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}})).await?;

            let t = mcp_http_post(&client, &url, &headers, &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})).await?;
            let tools = parse_tool_list(&t["result"], &config);
            tracing::info!(server=%name, url=%url, count=tools.len(), "MCP HTTP connected");
            ServerConn::Http { tools, url, headers, client, next_id: AtomicU64::new(3) }
        } else {
            let mut connection = McpConnection::handshake(config).await?;
            let stdin = tokio::io::BufWriter::new(connection.process.stdin.take().ok_or_else(|| anyhow!("stdin gone"))?);
            let stdout = BufReader::new(connection.process.stdout.take().ok_or_else(|| anyhow!("stdout gone"))?);
            ServerConn::Stdio { conn: connection, stdin, stdout }
        };

        self.servers.write().await.insert(name.clone(), conn);
        Ok(name)
    }

    pub async fn all_tool_definitions(&self) -> Result<Vec<ToolDefinition>> {
        let servers = self.servers.read().await;
        let mut defs = Vec::new();
        for (name, entry) in &*servers {
            for t in entry.tools() {
                defs.push(ToolDefinition {
                    name: format!("{}__{}__{}", self.tool_prefix, name, t.name),
                    description: format!("[MCP:{}] {}", name, t.description),
                    input_schema: t.input_schema.clone(),
                });
            }
        }
        Ok(defs)
    }

    pub async fn call_tool(&self, server: &str, tool: &str, args: Value) -> Result<Value> {
        let timeout = Duration::from_secs(default_tool_timeout());
        tokio::time::timeout(timeout, self.call_tool_inner(server, tool, args))
            .await
            .map_err(|_| anyhow!("MCP tool '{}::{}' timed out after {}s", server, tool, default_tool_timeout()))?
    }

    async fn call_tool_inner(&self, server: &str, tool: &str, args: Value) -> Result<Value> {
        let mut servers = self.servers.write().await;
        let entry = servers.get_mut(server).ok_or_else(|| anyhow!("MCP server '{}' not found", server))?;
        match entry {
            ServerConn::Stdio { conn, stdin, stdout } => {
                conn.send_request(stdin, stdout, "tools/call", &serde_json::json!({"name":tool,"arguments":args})).await
            }
            ServerConn::Http { url, headers, client, next_id, .. } => {
                let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = mcp_http_post(client, url, headers, &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":tool,"arguments":args}})).await?;
                match r.get("error") {
                    Some(e) => Err(anyhow!("MCP: {}", e["message"].as_str().unwrap_or("?"))),
                    None => Ok(r["result"].clone()),
                }
            }
        }
    }

    pub async fn server_names(&self) -> Vec<String> { self.servers.read().await.keys().cloned().collect() }
    pub async fn server_tools(&self, name: &str) -> Result<Vec<McpTool>> {
        Ok(self.servers.read().await.get(name).ok_or_else(|| anyhow!("not found: {}", name))?.tools().to_vec())
    }

    /// Check if a server connection is alive.
    pub async fn server_health(&self, name: &str) -> bool {
        self.servers.read().await.contains_key(name)
    }

    /// List resources from an MCP server (only HTTP supported currently).
    pub async fn list_resources(&self, server: &str) -> Result<Vec<McpResource>> {
        let mut servers = self.servers.write().await;
        let entry = servers.get_mut(server).ok_or_else(|| anyhow!("MCP server '{}' not found", server))?;
        match entry {
            ServerConn::Stdio { conn, stdin, stdout } => {
                let result = conn.send_request(stdin, stdout, "resources/list", &serde_json::json!({})).await?;
                let resources = result["resources"].as_array().map(|a| a.iter().filter_map(|r| {
                    Some(McpResource {
                        uri: r["uri"].as_str()?.to_string(),
                        name: r["name"].as_str()?.to_string(),
                        description: r["description"].as_str().map(String::from),
                        mime_type: r["mimeType"].as_str().map(String::from),
                        size: r["size"].as_u64(),
                    })
                }).collect()).unwrap_or_default();
                Ok(resources)
            }
            ServerConn::Http { url, headers, client, next_id, .. } => {
                let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = mcp_http_post(client, url, headers, &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"resources/list","params":{}})).await?;
                let result = r.get("result").ok_or_else(|| anyhow!("MCP: no result"))?;
                let resources = result["resources"].as_array().map(|a| a.iter().filter_map(|r| {
                    Some(McpResource {
                        uri: r["uri"].as_str()?.to_string(),
                        name: r["name"].as_str()?.to_string(),
                        description: r["description"].as_str().map(String::from),
                        mime_type: r["mimeType"].as_str().map(String::from),
                        size: r["size"].as_u64(),
                    })
                }).collect()).unwrap_or_default();
                Ok(resources)
            }
        }
    }

    /// Read a resource from an MCP server.
    pub async fn read_resource(&self, server: &str, uri: &str) -> Result<McpResourceContent> {
        let mut servers = self.servers.write().await;
        let entry = servers.get_mut(server).ok_or_else(|| anyhow!("MCP server '{}' not found", server))?;
        let timeout = default_tool_timeout();
        match entry {
            ServerConn::Stdio { conn, stdin, stdout } => {
                let result = tokio::time::timeout(
                    Duration::from_secs(timeout),
                    conn.send_request(stdin, stdout, "resources/read", &serde_json::json!({"uri": uri})),
                ).await.map_err(|_| anyhow!("MCP read_resource timeout after {}s", timeout))??;
                Ok(McpResourceContent {
                    uri: uri.to_string(),
                    mime_type: result["contents"][0]["mimeType"].as_str().map(String::from),
                    text: result["contents"][0]["text"].as_str().map(String::from),
                    blob: result["contents"][0]["blob"].as_str().map(String::from),
                })
            }
            ServerConn::Http { url, headers, client, next_id, .. } => {
                let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = tokio::time::timeout(
                    Duration::from_secs(timeout),
                    mcp_http_post(client, url, headers, &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"resources/read","params":{"uri":uri}})),
                ).await.map_err(|_| anyhow!("MCP read_resource timeout after {}s", timeout))??;
                let result = r.get("result").ok_or_else(|| anyhow!("MCP: no result"))?;
                Ok(McpResourceContent {
                    uri: uri.to_string(),
                    mime_type: result["contents"][0]["mimeType"].as_str().map(String::from),
                    text: result["contents"][0]["text"].as_str().map(String::from),
                    blob: result["contents"][0]["blob"].as_str().map(String::from),
                })
            }
        }
    }

    /// Disconnect and clean up an MCP server. For stdio transport, kills the child process.
    pub async fn disconnect(&self, name: &str) -> Result<()> {
        let mut servers = self.servers.write().await;
        if let Some(conn) = servers.remove(name) {
            match conn {
                ServerConn::Stdio { mut conn, .. } => {
                    conn.process.kill().await?;
                    tracing::info!(server=%name, "MCP stdio disconnected");
                }
                ServerConn::Http { .. } => {
                    tracing::info!(server=%name, "MCP HTTP disconnected");
                }
            }
        }
        Ok(())
    }

    /// Disconnect all servers.
    pub async fn disconnect_all(&self) -> Result<()> {
        let names = self.server_names().await;
        for name in names {
            let _ = self.disconnect(&name).await;
        }
        Ok(())
    }
}

impl Default for McpClient { fn default() -> Self { Self::new() } }

// ============================================================================
// HTTP helper — handles both JSON and SSE responses from MCP servers
// ============================================================================

async fn mcp_http_post(client: &reqwest::Client, url: &str, headers: &reqwest::header::HeaderMap, body: &Value) -> Result<Value> {
    let mut req_headers = headers.clone();
    req_headers.insert(reqwest::header::CONTENT_TYPE, reqwest::header::HeaderValue::from_static("application/json"));
    let resp = client.post(url).headers(req_headers).json(body).send().await?;
    let status = resp.status();
    let content_type = resp.headers().get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let text = resp.text().await?;

    if !status.is_success() {
        anyhow::bail!("MCP HTTP {}: {}", status.as_u16(), truncate(&text, 500));
    }
    tracing::debug!(%url, %status, content_type, body_len=text.len(), "MCP HTTP");

    if text.is_empty() {
        anyhow::bail!("MCP: empty response (status={}, type={})", status.as_u16(), content_type);
    }

    if content_type.contains("text/event-stream") {
        let mut result = Value::Null;
        for event in text.split("\n\n") {
            let mut event_type = ""; let mut data = "";
            for line in event.lines() {
                if let Some(t) = line.strip_prefix("event: ") { event_type = t.trim(); }
                if let Some(d) = line.strip_prefix("data: ") { data = d.trim(); }
            }
            if (event_type == "message" || event_type.is_empty()) && !data.is_empty() {
                if let Ok(parsed) = serde_json::from_str::<Value>(data) { result = parsed; }
            }
        }
        if result.is_null() { anyhow::bail!("SSE contained no data: {}", truncate(&text, 200)); }
        Ok(result)
    } else {
        serde_json::from_str(&text).map_err(|e| anyhow!("MCP parse: {} — body: {}", e, truncate(&text, 300)))
    }
}

async fn mcp_http_notify(client: &reqwest::Client, url: &str, headers: &reqwest::header::HeaderMap, body: &Value) -> Result<()> {
    let resp = client.post(url).headers(headers.clone()).json(body).send().await?;
    tracing::debug!(%url, status=%resp.status(), "MCP HTTP notify");
    Ok(())
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_config_defaults() {
        let config = McpServerConfig {
            name: "t".into(), command: "e".into(), args: vec![], env: HashMap::new(),
            transport: "".into(), url: None, headers: HashMap::new(), cwd: None,
            startup_timeout_secs: 30, tool_timeout_secs: 60,
            enabled_tools: None, disabled_tools: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        let _: McpServerConfig = serde_json::from_str(&json).unwrap();
    }
}
