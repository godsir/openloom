// SPDX-License-Identifier: Apache-2.0
//! MCP (Model Context Protocol) client for openLoom v2.
//!
//! Supports both stdio transport (spawning MCP server processes) and
//! HTTP transport (POST JSON-RPC 2.0 to an MCP endpoint).

use anyhow::{Context, Result, anyhow};
use loom_types::{
    GetPromptResult, McpPrompt, McpPromptMessage, McpResource, McpResourceContent,
    McpResourceTemplate, McpTool, ToolDefinition,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
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

fn default_transport() -> String {
    "stdio".into()
}
fn default_startup_timeout() -> u64 {
    30
}
fn default_tool_timeout() -> u64 {
    60
}

/// Resolve the effective per-call tool timeout: use the configured value,
/// falling back to the default when unset/zero.
fn resolve_tool_timeout(configured: u64) -> u64 {
    if configured == 0 {
        default_tool_timeout()
    } else {
        configured
    }
}

/// Resolve the effective startup/handshake timeout: use the configured value,
/// falling back to the default when unset/zero.
fn resolve_startup_timeout(configured: u64) -> u64 {
    if configured == 0 {
        default_startup_timeout()
    } else {
        configured
    }
}

// ============================================================================
// Stdio connection
// ============================================================================

struct McpConnection {
    process: Child,
    stdin: tokio::io::BufWriter<tokio::process::ChildStdin>,
    stdout: BufReader<tokio::process::ChildStdout>,
    tools: Vec<McpTool>,
    next_id: u64,
    /// Resolved per-tool-call timeout (seconds) for this server.
    tool_timeout_secs: u64,
}

/// On Windows, .sh files are not natively executable. If the command is a
/// .sh script, try to locate Git Bash or WSL to run it. Falls back to the
/// original command if no interpreter is found (may trigger the OS file
/// association dialog, but that's better than silently breaking).
///
/// Also on Windows, `.cmd`/`.bat` wrappers (npx, npm-global tools, etc.)
/// need `cmd /C` to launch reliably. Arguments are passed individually —
/// never joined into a single string (which would mangle quoted args).
fn prepare_command(raw_cmd: &str, raw_args: &[String]) -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        if raw_cmd.ends_with(".sh") {
            let bash = find_bash();
            if let Some(b) = bash {
                let mut args = vec![raw_cmd.to_string()];
                args.extend_from_slice(raw_args);
                return (b, args);
            }
        }
        if needs_cmd_wrapper(raw_cmd) {
            let mut args = vec!["/C".to_string(), raw_cmd.to_string()];
            args.extend(raw_args.iter().cloned());
            return ("cmd".to_string(), args);
        }
    }
    (raw_cmd.to_string(), raw_args.to_vec())
}

#[cfg(windows)]
fn needs_cmd_wrapper(cmd: &str) -> bool {
    let probe = cmd.split_whitespace().next().unwrap_or(cmd);
    if probe.ends_with(".cmd") || probe.ends_with(".bat") {
        return true;
    }
    // `where` returns all matches; if any is a .cmd/.bat, we must wrap.
    if let Ok(out) = std::process::Command::new("where").arg(probe).output()
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout);
        return s.lines().any(|l| {
            let l = l.trim().to_lowercase();
            l.ends_with(".cmd") || l.ends_with(".bat")
        });
    }
    false
}

#[cfg(windows)]
fn find_bash() -> Option<String> {
    let git_bash = r"C:\Program Files\Git\bin\bash.exe";
    if std::path::Path::new(git_bash).exists() {
        return Some(git_bash.to_string());
    }
    if let Ok(output) = std::process::Command::new("where").arg("wsl.exe").output()
        && output.status.success()
    {
        return Some("wsl.exe".to_string());
    }
    None
}

impl McpConnection {
    async fn handshake(config: McpServerConfig) -> Result<Self> {
        let (program, args) = prepare_command(&config.command, &config.args);
        let mut cmd = Command::new(&program);
        cmd.args(&args);

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        // ── Environment ──────────────────────────────────────────────
        // Inherit from parent process so that tools like npx/node resolve
        // correctly.  Only override with user-specified per-server env vars;
        // secrets isolation is the user's responsibility via mcp.json `env`.
        cmd.envs(std::env::vars());
        for (k, v) in &config.env {
            cmd.env(k, v);
        }
        if let Some(ref cwd) = config.cwd {
            cmd.current_dir(cwd);
        }

        let mut process = cmd
            .spawn()
            .with_context(|| format!("failed to spawn '{}'. Is it installed?", config.command))?;

        // Drain stderr to prevent deadlock, log to tracing for debugging.
        if let Some(stderr) = process.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                tracing::warn!(target: "mcp_stderr", "{}", trimmed);
                            }
                        }
                    }
                }
                tracing::info!(target: "mcp_stderr", "process stderr stream ended");
            });
        }

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| anyhow!("stdin unavailable"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow!("stdout unavailable"))?;
        let stdin = tokio::io::BufWriter::new(stdin);
        let stdout_reader = BufReader::new(stdout);
        let mut conn = Self {
            process,
            stdin,
            stdout: stdout_reader,
            tools: Vec::new(),
            next_id: 1,
            tool_timeout_secs: resolve_tool_timeout(config.tool_timeout_secs),
        };

        // Bound the whole init handshake (initialize + initialized + tools/list)
        // by the configured startup timeout so a hung server can't block forever.
        let startup = resolve_startup_timeout(config.startup_timeout_secs);
        tokio::time::timeout(Duration::from_secs(startup), conn.run_handshake(&config))
            .await
            .map_err(|_| {
                anyhow!(
                    "MCP server '{}' handshake timed out after {startup}s",
                    config.name
                )
            })??;

        Ok(conn)
    }

    /// Perform the JSON-RPC init handshake on an already-spawned connection.
    async fn run_handshake(&mut self, config: &McpServerConfig) -> Result<()> {
        // init
        let result = self
            .send_request(
                "initialize",
                &serde_json::json!({
                    "protocolVersion": "2024-11-05", "capabilities": {},
                    "clientInfo": {"name": "openLoom", "version": "0.2.0"}
                }),
            )
            .await?;
        tracing::info!(server=%config.name, version=%result["protocolVersion"], "MCP init");

        // initialized notification
        self.send_notification("notifications/initialized", &serde_json::json!({}))
            .await?;

        // tools/list
        let tools_resp = self
            .send_request("tools/list", &serde_json::json!({}))
            .await?;
        self.tools = parse_tool_list(&tools_resp, config);
        tracing::info!(server=%config.name, count=self.tools.len(), "MCP tools");

        Ok(())
    }

    async fn send_request(&mut self, method: &str, params: &Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let mut body = serde_json::to_string(
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )?;
        body.push('\n');
        self.stdin.write_all(body.as_bytes()).await?;
        self.stdin.flush().await?;

        // Read responses line by line. MCP servers may interleave
        // server-initiated notifications (no top-level `id`, e.g.
        // notifications/message, notifications/progress) and, rarely,
        // server->client requests (`id` + `method`) over stdout. Skip
        // anything that is not the response to our request `id`.
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.stdout.read_line(&mut line).await?;
            if n == 0 {
                return Err(anyhow!(
                    "MCP stdout closed before response to '{method}' (id={id}). Check backend logs (mcp_stderr target) for stderr output."
                ));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let msg: Value = serde_json::from_str(trimmed)
                .with_context(|| format!("MCP parse error: {}", truncate(trimmed, 200)))?;

            // A response to our request carries a matching `id` and no
            // `method`. A message with an `id` AND a `method` is a
            // server->client request: log and skip it.
            let msg_id = msg.get("id");
            let is_request = msg.get("method").is_some();
            match msg_id {
                Some(mid) if !is_request && json_id_eq(mid, id) => {
                    if let Some(err) = msg.get("error") {
                        return Err(anyhow!(
                            "MCP error: {}",
                            err["message"].as_str().unwrap_or("unknown")
                        ));
                    }
                    return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
                }
                Some(_) if is_request => {
                    tracing::debug!(
                        target: "mcp_stdio",
                        method = msg["method"].as_str().unwrap_or(""),
                        "skipping server->client request"
                    );
                }
                Some(other) => {
                    tracing::debug!(
                        target: "mcp_stdio",
                        got = %other,
                        want = id,
                        "skipping response with non-matching id"
                    );
                }
                None => {
                    // Notification (no `id`): logging, progress, list_changed, etc.
                    tracing::trace!(
                        target: "mcp_stdio",
                        method = msg["method"].as_str().unwrap_or(""),
                        "skipping notification"
                    );
                }
            }
        }
    }

    async fn send_notification(&mut self, method: &str, params: &Value) -> Result<()> {
        let mut body = serde_json::to_string(
            &serde_json::json!({"jsonrpc":"2.0","method":method,"params":params}),
        )?;
        body.push('\n');
        self.stdin.write_all(body.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }
}

fn parse_tool_list(result: &Value, config: &McpServerConfig) -> Vec<McpTool> {
    let tools = result["tools"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    tools
        .iter()
        .filter_map(|t| {
            let name = t["name"].as_str()?.to_string();
            if let Some(ref a) = config.enabled_tools
                && !a.contains(&name)
            {
                return None;
            }
            if let Some(ref d) = config.disabled_tools
                && d.contains(&name)
            {
                return None;
            }
            Some(McpTool {
                name,
                title: t["title"].as_str().map(String::from),
                description: t["description"].as_str().unwrap_or("").to_string(),
                input_schema: t["inputSchema"].clone(),
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            })
        })
        .collect()
}

// ============================================================================
// Connection types
// ============================================================================

enum ServerConn {
    Stdio {
        conn: Box<McpConnection>,
    },
    Http {
        tools: Vec<McpTool>,
        url: String,
        headers: reqwest::header::HeaderMap,
        client: reqwest::Client,
        next_id: AtomicU64,
        /// Resolved per-tool-call timeout (seconds) for this server.
        tool_timeout_secs: u64,
    },
}

impl ServerConn {
    fn tools(&self) -> &[McpTool] {
        match self {
            ServerConn::Stdio { conn, .. } => &conn.tools,
            ServerConn::Http { tools, .. } => tools,
        }
    }

    /// The resolved per-tool-call timeout (seconds) for this server.
    fn tool_timeout_secs(&self) -> u64 {
        match self {
            ServerConn::Stdio { conn, .. } => conn.tool_timeout_secs,
            ServerConn::Http {
                tool_timeout_secs, ..
            } => *tool_timeout_secs,
        }
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
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            tool_prefix: "mcp".into(),
        }
    }

    pub async fn connect(&self, config: McpServerConfig) -> Result<String> {
        let name = config.name.clone();
        let conn = if config.transport == "http" {
            let url = config
                .url
                .as_ref()
                .ok_or_else(|| anyhow!("HTTP transport needs 'url'"))?
                .trim_end_matches('/')
                .to_string();
            let mut headers = reqwest::header::HeaderMap::new();
            for (k, v) in &config.headers {
                headers.insert(
                    reqwest::header::HeaderName::from_bytes(k.as_bytes())
                        .map_err(|e| anyhow!("bad header '{k}': {e}"))?,
                    reqwest::header::HeaderValue::from_str(v)
                        .map_err(|e| anyhow!("bad value '{k}': {e}"))?,
                );
            }
            // MCP HTTP spec requires Accept: application/json, text/event-stream
            headers.insert(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/json, text/event-stream"),
            );
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .connect_timeout(Duration::from_secs(10))
                .build()?;

            // Bound the HTTP handshake by the configured startup timeout. (The
            // reqwest client also has its own request timeout above.)
            let startup = resolve_startup_timeout(config.startup_timeout_secs);
            let r = tokio::time::timeout(
                Duration::from_secs(startup),
                mcp_http_post(&client, &url, &headers, &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"openLoom","version":"0.2.0"}}})),
            )
            .await
            .map_err(|_| anyhow!("MCP server '{name}' handshake timed out after {startup}s"))??;
            if r.get("error").is_some() {
                anyhow::bail!(
                    "MCP init: {}",
                    r["error"]["message"].as_str().unwrap_or("?")
                );
            }

            mcp_http_notify(&client, &url, &headers, &serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}})).await?;

            let t = tokio::time::timeout(
                Duration::from_secs(startup),
                mcp_http_post(
                    &client,
                    &url,
                    &headers,
                    &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
                ),
            )
            .await
            .map_err(|_| anyhow!("MCP server '{name}' tools/list timed out after {startup}s"))??;
            let tools = parse_tool_list(&t["result"], &config);
            tracing::info!(server=%name, url=%url, count=tools.len(), "MCP HTTP connected");
            ServerConn::Http {
                tools,
                url,
                headers,
                client,
                next_id: AtomicU64::new(3),
                tool_timeout_secs: resolve_tool_timeout(config.tool_timeout_secs),
            }
        } else {
            let connection = McpConnection::handshake(config).await?;
            ServerConn::Stdio {
                conn: Box::new(connection),
            }
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
                    tags: vec![],
                });
            }
        }
        Ok(defs)
    }

    pub async fn call_tool(&self, server: &str, tool: &str, args: Value) -> Result<Value> {
        // Resolve the per-server tool timeout under a brief read lock so the
        // lock is released before we take the write lock in call_tool_inner.
        let timeout_secs = {
            let servers = self.servers.read().await;
            servers
                .get(server)
                .map(ServerConn::tool_timeout_secs)
                .unwrap_or_else(default_tool_timeout)
        };
        let timeout = Duration::from_secs(timeout_secs);
        tokio::time::timeout(timeout, self.call_tool_inner(server, tool, args))
            .await
            .map_err(|_| anyhow!("MCP tool '{server}::{tool}' timed out after {timeout_secs}s"))?
    }

    async fn call_tool_inner(&self, server: &str, tool: &str, args: Value) -> Result<Value> {
        // HTTP transport: snapshot the cheap-to-clone request pieces (client is
        // Arc-backed, url/headers are owned) and reserve a request id under a
        // BRIEF read lock, then release it and perform the network call with no
        // lock held — so a slow HTTP tool never blocks other servers.
        //
        // Stdio transport shares one pair of stdin/stdout pipes per server, so
        // requests genuinely must be serialized: that path keeps the write lock
        // for the duration of the call (handled below).
        let http_req = {
            let servers = self.servers.read().await;
            let entry = servers
                .get(server)
                .ok_or_else(|| anyhow!("MCP server '{server}' not found"))?;
            match entry {
                ServerConn::Http {
                    url,
                    headers,
                    client,
                    next_id,
                    ..
                } => {
                    let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Some((client.clone(), url.clone(), headers.clone(), id))
                }
                // Stdio is handled under the write lock below.
                ServerConn::Stdio { .. } => None,
            }
        };

        if let Some((client, url, headers, id)) = http_req {
            let r = mcp_http_post(
                &client,
                &url,
                &headers,
                &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":tool,"arguments":args}}),
            )
            .await?;
            return match r.get("error") {
                Some(e) => Err(anyhow!("MCP: {}", e["message"].as_str().unwrap_or("?"))),
                None => Ok(r["result"].clone()),
            };
        }

        // Stdio transport: serialize on the shared pipes via the write lock.
        let mut servers = self.servers.write().await;
        let entry = servers
            .get_mut(server)
            .ok_or_else(|| anyhow!("MCP server '{server}' not found"))?;
        match entry {
            ServerConn::Stdio { conn } => {
                conn.send_request(
                    "tools/call",
                    &serde_json::json!({"name":tool,"arguments":args}),
                )
                .await
            }
            // HTTP is normally handled above without holding the lock. We only
            // reach here if the server entry changed transport between the two
            // lock acquisitions (a rare reconnect race). Handle it correctly
            // rather than panicking; the lock is held only for this one call.
            ServerConn::Http {
                url,
                headers,
                client,
                next_id,
                ..
            } => {
                let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = mcp_http_post(client, url, headers, &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":tool,"arguments":args}})).await?;
                match r.get("error") {
                    Some(e) => Err(anyhow!("MCP: {}", e["message"].as_str().unwrap_or("?"))),
                    None => Ok(r["result"].clone()),
                }
            }
        }
    }

    pub async fn server_names(&self) -> Vec<String> {
        self.servers.read().await.keys().cloned().collect()
    }
    pub async fn server_tools(&self, name: &str) -> Result<Vec<McpTool>> {
        Ok(self
            .servers
            .read()
            .await
            .get(name)
            .ok_or_else(|| anyhow!("not found: {name}"))?
            .tools()
            .to_vec())
    }

    /// Check if a server connection is alive.
    pub async fn server_health(&self, name: &str) -> bool {
        self.servers.read().await.contains_key(name)
    }

    /// List resources from an MCP server (only HTTP supported currently).
    pub async fn list_resources(&self, server: &str) -> Result<Vec<McpResource>> {
        let mut servers = self.servers.write().await;
        let entry = servers
            .get_mut(server)
            .ok_or_else(|| anyhow!("MCP server '{server}' not found"))?;
        match entry {
            ServerConn::Stdio { conn } => {
                let result = conn
                    .send_request("resources/list", &serde_json::json!({}))
                    .await?;
                let resources = result["resources"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|r| {
                                Some(McpResource {
                                    uri: r["uri"].as_str()?.to_string(),
                                    name: r["name"].as_str()?.to_string(),
                                    description: r["description"].as_str().map(String::from),
                                    mime_type: r["mimeType"].as_str().map(String::from),
                                    size: r["size"].as_u64(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(resources)
            }
            ServerConn::Http {
                url,
                headers,
                client,
                next_id,
                ..
            } => {
                let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = mcp_http_post(client, url, headers, &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"resources/list","params":{}})).await?;
                let result = r.get("result").ok_or_else(|| anyhow!("MCP: no result"))?;
                let resources = result["resources"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|r| {
                                Some(McpResource {
                                    uri: r["uri"].as_str()?.to_string(),
                                    name: r["name"].as_str()?.to_string(),
                                    description: r["description"].as_str().map(String::from),
                                    mime_type: r["mimeType"].as_str().map(String::from),
                                    size: r["size"].as_u64(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(resources)
            }
        }
    }

    /// Read a resource from an MCP server. Returns all content blocks.
    pub async fn read_resource(&self, server: &str, uri: &str) -> Result<Vec<McpResourceContent>> {
        let mut servers = self.servers.write().await;
        let entry = servers
            .get_mut(server)
            .ok_or_else(|| anyhow!("MCP server '{server}' not found"))?;
        let timeout = entry.tool_timeout_secs();
        match entry {
            ServerConn::Stdio { conn } => {
                let result = tokio::time::timeout(
                    Duration::from_secs(timeout),
                    conn.send_request("resources/read", &serde_json::json!({"uri": uri})),
                )
                .await
                .map_err(|_| anyhow!("MCP read_resource timeout after {timeout}s"))??;
                Ok(parse_resource_contents(&result, uri))
            }
            ServerConn::Http {
                url,
                headers,
                client,
                next_id,
                ..
            } => {
                let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = tokio::time::timeout(
                    Duration::from_secs(timeout),
                    mcp_http_post(client, url, headers, &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"resources/read","params":{"uri":uri}})),
                ).await.map_err(|_| anyhow!("MCP read_resource timeout after {timeout}s"))??;
                let result = r.get("result").ok_or_else(|| anyhow!("MCP: no result"))?;
                Ok(parse_resource_contents(result, uri))
            }
        }
    }

    /// List resource templates from an MCP server.
    pub async fn list_resource_templates(&self, server: &str) -> Result<Vec<McpResourceTemplate>> {
        let mut servers = self.servers.write().await;
        let entry = servers
            .get_mut(server)
            .ok_or_else(|| anyhow!("MCP server '{server}' not found"))?;
        match entry {
            ServerConn::Stdio { conn } => {
                let result = conn
                    .send_request("resources/templates/list", &serde_json::json!({}))
                    .await?;
                Ok(parse_resource_templates(&result))
            }
            ServerConn::Http {
                url,
                headers,
                client,
                next_id,
                ..
            } => {
                let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = mcp_http_post(client, url, headers, &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"resources/templates/list","params":{}})).await?;
                let result = r.get("result").ok_or_else(|| anyhow!("MCP: no result"))?;
                Ok(parse_resource_templates(result))
            }
        }
    }

    /// List prompts from an MCP server.
    pub async fn list_prompts(&self, server: &str) -> Result<Vec<McpPrompt>> {
        let mut servers = self.servers.write().await;
        let entry = servers
            .get_mut(server)
            .ok_or_else(|| anyhow!("MCP server '{server}' not found"))?;
        match entry {
            ServerConn::Stdio { conn } => {
                let result = conn
                    .send_request("prompts/list", &serde_json::json!({}))
                    .await?;
                Ok(parse_prompt_list(&result))
            }
            ServerConn::Http {
                url,
                headers,
                client,
                next_id,
                ..
            } => {
                let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = mcp_http_post(client, url, headers, &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"prompts/list","params":{}})).await?;
                let result = r.get("result").ok_or_else(|| anyhow!("MCP: no result"))?;
                Ok(parse_prompt_list(result))
            }
        }
    }

    /// Get a prompt from an MCP server with arguments.
    pub async fn get_prompt(
        &self,
        server: &str,
        name: &str,
        arguments: Option<&serde_json::Value>,
    ) -> Result<GetPromptResult> {
        let mut servers = self.servers.write().await;
        let entry = servers
            .get_mut(server)
            .ok_or_else(|| anyhow!("MCP server '{server}' not found"))?;
        let params = if let Some(args) = arguments {
            serde_json::json!({"name": name, "arguments": args})
        } else {
            serde_json::json!({"name": name})
        };
        match entry {
            ServerConn::Stdio { conn } => {
                let result = conn.send_request("prompts/get", &params).await?;
                Ok(parse_get_prompt_result(&result))
            }
            ServerConn::Http {
                url,
                headers,
                client,
                next_id,
                ..
            } => {
                let id = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = mcp_http_post(client, url, headers, &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"prompts/get","params":params})).await?;
                let result = r.get("result").ok_or_else(|| anyhow!("MCP: no result"))?;
                Ok(parse_get_prompt_result(result))
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
                    conn.process.wait().await?;
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

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// HTTP helper — handles both JSON and SSE responses from MCP servers
// ============================================================================

async fn mcp_http_post(
    client: &reqwest::Client,
    url: &str,
    headers: &reqwest::header::HeaderMap,
    body: &Value,
) -> Result<Value> {
    let mut req_headers = headers.clone();
    req_headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    let resp = client
        .post(url)
        .headers(req_headers)
        .json(body)
        .send()
        .await?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let text = resp.text().await?;

    if !status.is_success() {
        anyhow::bail!("MCP HTTP {}: {}", status.as_u16(), truncate(&text, 500));
    }
    tracing::debug!(%url, %status, content_type, body_len=text.len(), "MCP HTTP");

    if text.is_empty() {
        anyhow::bail!(
            "MCP: empty response (status={}, type={})",
            status.as_u16(),
            content_type
        );
    }

    if content_type.contains("text/event-stream") {
        // Match the response to our request id; falls back to the first event
        // that looks like a JSON-RPC response (notifications carry no id).
        parse_sse_response(&text, body.get("id"))
    } else {
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("MCP parse: {} — body: {}", e, truncate(&text, 300)))
    }
}

async fn mcp_http_notify(
    client: &reqwest::Client,
    url: &str,
    headers: &reqwest::header::HeaderMap,
    body: &Value,
) -> Result<()> {
    let resp = client
        .post(url)
        .headers(headers.clone())
        .json(body)
        .send()
        .await?;
    tracing::debug!(%url, status=%resp.status(), "MCP HTTP notify");
    let _ = resp.bytes().await;
    Ok(())
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Parse a `text/event-stream` MCP response body.
///
/// Splits the body into SSE events (separated by a blank line). Within each
/// event, all `data:` lines are concatenated with `\n` (a single SSE event's
/// payload may span multiple `data:` lines); `:`-comment/keepalive lines and
/// non-`data` fields (`event:`, `id:`, `retry:`) are ignored. Returns the
/// first event whose parsed JSON `id` matches `want_id`; otherwise the first
/// event that looks like a JSON-RPC response (has `result`/`error`).
fn parse_sse_response(text: &str, want_id: Option<&Value>) -> Result<Value> {
    let mut fallback = Value::Null;
    let normalized = text.replace("\r\n", "\n");
    for event in normalized.split("\n\n") {
        let mut data_parts: Vec<&str> = Vec::new();
        for line in event.lines() {
            if line.starts_with(':') {
                continue; // comment / keepalive
            }
            // Accept "data:" with or without a leading space (per SSE spec a
            // single optional space after the colon is stripped).
            if let Some(rest) = line.strip_prefix("data:") {
                data_parts.push(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
        if data_parts.is_empty() {
            continue;
        }
        let data = data_parts.join("\n");
        let parsed: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Return the first event whose JSON matches our request id.
        if let Some(want) = want_id
            && parsed.get("id").map(|got| got == want).unwrap_or(false)
        {
            return Ok(parsed);
        }
        // Otherwise remember the first event that at least looks like a
        // JSON-RPC response (has result or error) as a fallback.
        if fallback.is_null()
            && (parsed.get("result").is_some() || parsed.get("error").is_some())
        {
            fallback = parsed;
        }
    }
    if fallback.is_null() {
        anyhow::bail!("SSE contained no matching data: {}", truncate(text, 200));
    }
    Ok(fallback)
}

/// Compare a JSON-RPC response `id` (as received) against the numeric id we
/// sent. Spec-conformant servers echo the number, but some echo it as a
/// string; accept either form.
fn json_id_eq(received: &Value, sent: u64) -> bool {
    match received {
        Value::Number(n) => n.as_u64() == Some(sent),
        Value::String(s) => s.parse::<u64>().map(|v| v == sent).unwrap_or(false),
        _ => false,
    }
}

fn parse_resource_contents(result: &Value, uri: &str) -> Vec<McpResourceContent> {
    result["contents"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| McpResourceContent {
                    uri: c["uri"].as_str().unwrap_or(uri).to_string(),
                    mime_type: c["mimeType"].as_str().map(String::from),
                    text: c["text"].as_str().map(String::from),
                    blob: c["blob"].as_str().map(String::from),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_resource_templates(result: &Value) -> Vec<McpResourceTemplate> {
    result["resourceTemplates"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    Some(McpResourceTemplate {
                        uri_template: t["uriTemplate"].as_str()?.to_string(),
                        name: t["name"].as_str()?.to_string(),
                        description: t["description"].as_str().map(String::from),
                        mime_type: t["mimeType"].as_str().map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_prompt_list(result: &Value) -> Vec<McpPrompt> {
    result["prompts"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|p| McpPrompt {
                    name: p["name"].as_str().unwrap_or("").to_string(),
                    description: p["description"].as_str().map(String::from),
                    arguments: p["arguments"]
                        .as_array()
                        .map(|args| {
                            args.iter()
                                .map(|a| loom_types::McpPromptArgument {
                                    name: a["name"].as_str().unwrap_or("").to_string(),
                                    description: a["description"].as_str().map(String::from),
                                    required: a["required"].as_bool().unwrap_or(false),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_get_prompt_result(result: &Value) -> GetPromptResult {
    let messages = result["messages"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|m| McpPromptMessage {
                    role: m["role"].as_str().unwrap_or("user").to_string(),
                    content: parse_content_block(&m["content"]),
                })
                .collect()
        })
        .unwrap_or_default();
    GetPromptResult {
        description: result["description"].as_str().map(String::from),
        messages,
    }
}

fn parse_content_block(value: &Value) -> loom_types::McpContentBlock {
    match value["type"].as_str() {
        Some("text") => loom_types::McpContentBlock::Text {
            text: value["text"].as_str().unwrap_or("").to_string(),
        },
        Some("image") => loom_types::McpContentBlock::Image {
            data: value["data"].as_str().unwrap_or("").to_string(),
            mime_type: value["mimeType"]
                .as_str()
                .unwrap_or("image/png")
                .to_string(),
        },
        Some("resource") => loom_types::McpContentBlock::Resource {
            resource: McpResourceContent {
                uri: value["resource"]["uri"].as_str().unwrap_or("").to_string(),
                mime_type: value["resource"]["mimeType"].as_str().map(String::from),
                text: value["resource"]["text"].as_str().map(String::from),
                blob: value["resource"]["blob"].as_str().map(String::from),
            },
        },
        _ => loom_types::McpContentBlock::Text {
            text: String::new(),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    #[test]
    fn test_config_defaults() {
        let config = McpServerConfig {
            name: "t".into(),
            command: "e".into(),
            args: vec![],
            env: HashMap::new(),
            transport: "".into(),
            url: None,
            headers: HashMap::new(),
            cwd: None,
            startup_timeout_secs: 30,
            tool_timeout_secs: 60,
            enabled_tools: None,
            disabled_tools: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        let _: McpServerConfig = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_json_id_eq() {
        assert!(json_id_eq(&serde_json::json!(7), 7));
        assert!(!json_id_eq(&serde_json::json!(8), 7));
        // Some servers echo the id as a string.
        assert!(json_id_eq(&serde_json::json!("7"), 7));
        assert!(!json_id_eq(&serde_json::json!("x"), 7));
        // Null / wrong-type ids never match.
        assert!(!json_id_eq(&Value::Null, 7));
    }

    #[test]
    fn test_resolve_timeouts_fall_back_on_zero() {
        assert_eq!(resolve_tool_timeout(0), default_tool_timeout());
        assert_eq!(resolve_tool_timeout(120), 120);
        assert_eq!(resolve_startup_timeout(0), default_startup_timeout());
        assert_eq!(resolve_startup_timeout(15), 15);
    }

    #[test]
    fn test_sse_matches_by_id_and_skips_notification() {
        // Notification (no id) precedes the real response; we must skip it and
        // return the event whose id matches the request id (2).
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{}}\n\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"ok\":true}}\n\n";
        let got = parse_sse_response(body, Some(&serde_json::json!(2))).unwrap();
        assert_eq!(got["id"], serde_json::json!(2));
        assert_eq!(got["result"]["ok"], serde_json::json!(true));
    }

    #[test]
    fn test_sse_concatenates_multiline_data() {
        // A single event whose JSON payload is split across two data: lines
        // (each at column 0, joined with '\n').
        let body = "data: {\"jsonrpc\":\"2.0\",\"id\":5,\ndata: \"result\":{\"v\":1}}\n\n";
        let got = parse_sse_response(body, Some(&serde_json::json!(5))).unwrap();
        assert_eq!(got["id"], serde_json::json!(5));
        assert_eq!(got["result"]["v"], serde_json::json!(1));
    }

    #[test]
    fn test_sse_ignores_comments_and_falls_back_without_id() {
        // Keepalive comment lines are ignored; with no matching id we fall back
        // to the first event that has a result/error.
        let body = ": keepalive\n\ndata: {\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{}}\n\n";
        let got = parse_sse_response(body, None).unwrap();
        assert_eq!(got["id"], serde_json::json!(9));
    }

    #[test]
    fn test_sse_no_data_errors() {
        let body = ": only a comment\n\nevent: ping\n\n";
        assert!(parse_sse_response(body, Some(&serde_json::json!(1))).is_err());
    }
}
