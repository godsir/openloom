//! LSP (Language Server Protocol) client for openLoom v2.
//!
//! Manages language server processes via stdio JSON-RPC, supporting
//! diagnostics, completion, hover, definition, references, and document symbols.

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

const MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const LSP_TIMEOUT_SECS: u64 = 30;
/// Maximum number of documents kept open across all language servers. When the
/// cap is exceeded the least-recently-used document is closed (`didClose`) so
/// the corresponding server can release it. It is re-opened on next use.
const MAX_OPEN_FILES: usize = 128;

// ============================================================================
// LSP Connection 鈥?single language server process
// ============================================================================

/// Push-diagnostics store: maps a document URI to its latest list of
/// `textDocument/publishDiagnostics` `diagnostics` arrays.
type DiagnosticsStore = Arc<StdMutex<HashMap<String, Vec<Value>>>>;

struct LspConnection {
    #[allow(dead_code)]
    process: Child,
    next_id: AtomicU64,
}

impl LspConnection {
    fn new(process: Child) -> Self {
        Self {
            process,
            next_id: AtomicU64::new(1),
        }
    }

    /// Read one Content-Length framed message body from `reader`.
    /// Returns the raw bytes of the JSON body.
    async fn read_frame(
        reader: &mut BufReader<tokio::process::ChildStdout>,
    ) -> Result<Vec<u8>> {
        // Read headers up to the blank separator line.
        let mut header = String::new();
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                return Err(anyhow!("LSP process closed its stdout before responding — the language server may have crashed on startup. Check that the command and its dependencies are correctly installed."));
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            header.push_str(&line);
        }

        let content_len = header
            .lines()
            .find_map(|l| {
                l.strip_prefix("Content-Length: ")?
                    .trim()
                    .parse::<usize>()
                    .ok()
            })
            .ok_or_else(|| anyhow!("LSP: missing Content-Length header"))?;

        if content_len > MAX_BODY_SIZE {
            return Err(anyhow!(
                "LSP: response body too large ({content_len} bytes, max {MAX_BODY_SIZE})"
            ));
        }

        let mut body_bytes = vec![0u8; content_len];
        tokio::io::AsyncReadExt::read_exact(reader, &mut body_bytes).await?;
        Ok(body_bytes)
    }

    /// Parse a framed body into JSON, with a safe UTF-8 truncated preview on error.
    fn parse_frame(body_bytes: &[u8]) -> Result<Value> {
        serde_json::from_slice(body_bytes).with_context(|| {
            // Safe UTF-8 truncation at char boundary
            let content_len = body_bytes.len();
            let byte_limit = 200.min(content_len);
            let mut end = byte_limit;
            while end > 0 && end < content_len {
                if body_bytes[end] < 128 || body_bytes[end] & 0xC0 == 0xC0 {
                    break;
                }
                end -= 1;
            }
            let preview = String::from_utf8_lossy(&body_bytes[..end]);
            format!("LSP parse error: {preview}")
        })
    }

    async fn send_request(
        &self,
        writer: &mut BufWriter<tokio::process::ChildStdin>,
        reader: &mut BufReader<tokio::process::ChildStdout>,
        diagnostics: &DiagnosticsStore,
        method: &str,
        params: &Value,
    ) -> Result<Value> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let body = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;

        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        writer.write_all(frame.as_bytes()).await?;
        writer.flush().await?;

        // Loop reading framed messages until our matching response arrives.
        // Server-initiated traffic (notifications, server->client requests) is
        // handled inline so the JSON-RPC stream never desyncs.
        loop {
            let body_bytes = Self::read_frame(reader).await?;
            let msg = Self::parse_frame(&body_bytes)?;

            let msg_method = msg.get("method").and_then(Value::as_str);
            let msg_id = msg.get("id");

            // Case 1: a response (has `id`, no `method`).
            if msg_method.is_none() {
                let is_ours = match msg_id {
                    Some(Value::Number(n)) => n.as_u64() == Some(id),
                    Some(Value::String(s)) => s.parse::<u64>().ok() == Some(id),
                    _ => false,
                };
                if is_ours {
                    if let Some(err) = msg.get("error") {
                        let m = err
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown");
                        return Err(anyhow!("LSP error: {m}"));
                    }
                    return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
                }
                // A response to some other (e.g. earlier timed-out) request; skip.
                tracing::trace!(target: "lsp", ?msg_id, "LSP: ignoring stray response");
                continue;
            }

            // From here on, `method` is present.
            let method_name = msg_method.unwrap_or("");

            // Case 2: a server->client request (has `id` AND `method`).
            if let Some(req_id) = msg_id {
                tracing::trace!(target: "lsp", method = method_name, "LSP: answering server request with null");
                let reply = serde_json::to_string(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": Value::Null,
                }))?;
                let reply_frame = format!("Content-Length: {}\r\n\r\n{}", reply.len(), reply);
                writer.write_all(reply_frame.as_bytes()).await?;
                writer.flush().await?;
                continue;
            }

            // Case 3: a notification (has `method`, no `id`).
            if method_name == "textDocument/publishDiagnostics" {
                if let Some(params) = msg.get("params")
                    && let Some(uri) = params.get("uri").and_then(Value::as_str)
                {
                    let diags = params
                        .get("diagnostics")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    if let Ok(mut store) = diagnostics.lock() {
                        store.insert(uri.to_string(), diags);
                    }
                }
            } else {
                tracing::trace!(target: "lsp", method = method_name, "LSP: ignoring notification");
            }
            // Keep looping until our response arrives.
        }
    }

    async fn send_notification(
        &self,
        writer: &mut BufWriter<tokio::process::ChildStdin>,
        method: &str,
        params: &Value,
    ) -> Result<()> {
        let body = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        writer.write_all(frame.as_bytes()).await?;
        writer.flush().await?;
        Ok(())
    }
}

// ============================================================================
// Server entry 鈥?wraps an LspConnection with its I/O handles and metadata
// ============================================================================

struct ServerEntry {
    conn: Box<LspConnection>,
    stdin: tokio::sync::Mutex<BufWriter<tokio::process::ChildStdin>>,
    stdout: tokio::sync::Mutex<BufReader<tokio::process::ChildStdout>>,
    language_id: String,
    doc_version: AtomicU64,
    /// Latest push diagnostics (`textDocument/publishDiagnostics`) keyed by URI.
    diagnostics: DiagnosticsStore,
    /// Ring buffer of recent stderr lines — surfaced in error messages when
    /// the language server crashes during a request.
    stderr_buf: Arc<StdMutex<VecDeque<String>>>,
}

impl ServerEntry {
    fn stderr_tail(&self) -> String {
        self.stderr_buf
            .lock()
            .ok()
            .map(|g| g.iter().cloned().collect::<Vec<_>>().join("\n"))
            .unwrap_or_default()
    }

    async fn request(&self, method: &str, params: &Value) -> Result<Value> {
        let result = tokio::time::timeout(Duration::from_secs(LSP_TIMEOUT_SECS), async {
            let mut stdin = self.stdin.lock().await;
            let mut stdout = self.stdout.lock().await;
            self.conn
                .send_request(&mut stdin, &mut stdout, &self.diagnostics, method, params)
                .await
        })
        .await;

        match result {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => {
                let stderr = self.stderr_tail();
                Err(anyhow!("{e}{}", stderr_suffix(&stderr)))
            }
            Err(_) => {
                let stderr = self.stderr_tail();
                Err(anyhow!(
                    "LSP request '{method}' timed out after {LSP_TIMEOUT_SECS}s{}",
                    stderr_suffix(&stderr)
                ))
            }
        }
    }

    async fn notify(&self, method: &str, params: &Value) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        self.conn
            .send_notification(&mut stdin, method, params)
            .await
    }
}

fn stderr_suffix(stderr: &str) -> String {
    if stderr.is_empty() {
        String::new()
    } else {
        format!("\n--- language server stderr ---\n{stderr}")
    }
}

// ============================================================================
// Language detection and default server commands
// ============================================================================

/// Check whether a command binary exists on the system PATH.
pub fn binary_available(command: &str) -> bool {
    #[cfg(windows)]
    {
        // Split on space for commands like "haskell-language-server-wrapper --lsp"
        let prog = command.split_whitespace().next().unwrap_or(command);
        // Try .exe, .cmd, .bat variants via `where`
        if let Ok(out) = std::process::Command::new("where").arg(prog).output() {
            return out.status.success();
        }
        // Also try to check raw command via command search
        std::process::Command::new("cmd")
            .args(["/C", "where", prog])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let prog = command.split_whitespace().next().unwrap_or(command);
        std::process::Command::new("which")
            .arg(prog)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// On Windows, `.cmd`/`.bat` wrappers (typescript-language-server, etc.) need
/// `cmd /C` to spawn reliably — `CreateProcess` alone can't interpret them.
/// Arguments are passed individually (never joined into one string).
/// This mirrors `loom_mcp::prepare_command` so LSP and MCP spawn identically.
fn prepare_command(raw_cmd: &str, raw_args: &[String]) -> (String, Vec<String>) {
    #[cfg(windows)]
    {
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

/// Return an appropriate install hint for a language server command.
pub fn install_hint(language: &str, _command: &str) -> Option<(&'static str, &'static str)> {
    match language {
        "rust" => Some(("rustup", "rustup component add rust-analyzer")),
        "typescript" | "javascript" => Some(("npm", "npm install -g typescript typescript-language-server")),
        "python" => Some(("pip", "pip install python-lsp-server")),
        "go" => Some(("go", "go install golang.org/x/tools/gopls@latest")),
        "c" | "cpp" => Some(("scoop", "scoop install llvm  # provides clangd")),
        "java" => Some(("scoop", "scoop install jdtls")),
        "csharp" => Some(("dotnet", "dotnet tool install -g OmniSharp")),
        "swift" => Some(("xcode", "Xcode includes sourcekit-lsp")),
        "kotlin" => Some(("scoop", r#"scoop install kotlin-language-server"#)),
        "scala" => Some(("cs", "cs install metals")),
        "ruby" => Some(("gem", "gem install solargraph")),
        "lua" => Some(("scoop", "scoop install lua-language-server")),
        "zig" => Some(("scoop", "scoop install zls")),
        "haskell" => Some(("ghcup", "ghcup install hls")),
        "dart" => Some(("dart", "dart pub global activate dart_language_server")),
        "vue" => Some(("npm", "npm install -g @vue/language-server")),
        "svelte" => Some(("npm", "npm install -g svelte-language-server")),
        "html" => Some(("npm", "npm install -g vscode-langservers-extracted")),
        "css" => Some(("npm", "npm install -g vscode-langservers-extracted")),
        "json" => Some(("npm", "npm install -g vscode-langservers-extracted")),
        "yaml" => Some(("npm", "npm install -g yaml-language-server")),
        "toml" => Some(("cargo", "cargo install taplo-cli --features lsp")),
        "markdown" => Some(("scoop", "scoop install marksman")),
        "bash" => Some(("npm", "npm install -g bash-language-server")),
        "dockerfile" => Some(("npm", "npm install -g dockerfile-language-server-nodejs")),
        _ => None,
    }
}

/// Return an uninstall command for a language server, if one exists.
/// Returns None for servers that have no clean uninstall path
/// (e.g. `go install` products, scoop-managed clangd).
pub fn uninstall_hint(language: &str) -> Option<&'static str> {
    match language {
        "rust" => Some("rustup component remove rust-analyzer"),
        "typescript" | "javascript" => Some("npm uninstall -g typescript typescript-language-server"),
        "python" => Some("pip uninstall -y python-lsp-server"),
        "csharp" => Some("dotnet tool uninstall -g OmniSharp"),
        "ruby" => Some("gem uninstall solargraph"),
        "vue" => Some("npm uninstall -g @vue/language-server"),
        "svelte" => Some("npm uninstall -g svelte-language-server"),
        "html" | "css" | "json" => Some("npm uninstall -g vscode-langservers-extracted"),
        "yaml" => Some("npm uninstall -g yaml-language-server"),
        "toml" => Some("cargo uninstall taplo-cli"),
        "bash" => Some("npm uninstall -g bash-language-server"),
        "dockerfile" => Some("npm uninstall -g dockerfile-language-server-nodejs"),
        // go (go install), c/cpp (scoop llvm), java (scoop jdtls), swift (xcode),
        // kotlin/scala/lua/zig/haskell/dart/markdown — no clean single-command uninstall.
        _ => None,
    }
}

fn language_config(ext: &str) -> Option<(&'static str, &'static str, Vec<&'static str>)> {
    match ext {
        "rs" => Some(("rust", "rust-analyzer", vec![])),
        "ts" | "tsx" => Some(("typescript", "typescript-language-server", vec!["--stdio"])),
        "js" | "jsx" | "mjs" | "cjs" => {
            Some(("javascript", "typescript-language-server", vec!["--stdio"]))
        }
        "py" | "pyi" => Some(("python", "pylsp", vec![])),
        "go" => Some(("go", "gopls", vec![])),
        "c" | "h" => Some(("c", "clangd", vec![])),
        "cpp" | "hpp" | "cc" | "cxx" | "hxx" => Some(("cpp", "clangd", vec![])),
        "java" => Some(("java", "jdtls", vec![])),
        "cs" => Some(("csharp", "omnisharp", vec!["-lsp"])),
        "swift" => Some(("swift", "sourcekit-lsp", vec![])),
        "kt" | "kts" => Some(("kotlin", "kotlin-language-server", vec![])),
        "scala" => Some(("scala", "metals", vec![])),
        "rb" => Some(("ruby", "solargraph", vec!["stdio"])),
        "lua" => Some(("lua", "lua-language-server", vec![])),
        "zig" => Some(("zig", "zls", vec![])),
        "hs" => Some(("haskell", "haskell-language-server-wrapper", vec!["--lsp"])),
        "elm" => Some(("elm", "elm-language-server", vec![])),
        "dart" => Some(("dart", "dart", vec!["language-server"])),
        "sql" => Some((
            "sql",
            "sql-language-server",
            vec!["up", "--method", "stdio"],
        )),
        "vue" => Some(("vue", "vue-language-server", vec!["--stdio"])),
        "svelte" => Some(("svelte", "svelteserver", vec!["--stdio"])),
        "astro" => Some(("astro", "astro-ls", vec!["--stdio"])),
        "html" | "htm" => Some(("html", "vscode-html-language-server", vec!["--stdio"])),
        "css" | "scss" | "less" => Some(("css", "vscode-css-language-server", vec!["--stdio"])),
        "json" | "jsonc" => Some(("json", "vscode-json-language-server", vec!["--stdio"])),
        "yaml" | "yml" => Some(("yaml", "yaml-language-server", vec!["--stdio"])),
        "toml" => Some(("toml", "taplo", vec!["lsp", "stdio"])),
        "md" | "mdx" => Some(("markdown", "marksman", vec!["server"])),
        "sh" | "bash" | "zsh" => Some(("bash", "bash-language-server", vec!["start"])),
        "nix" => Some(("nix", "nil", vec![])),
        "tf" | "tfvars" | "hcl" => Some(("terraform", "terraform-ls", vec!["serve"])),
        "dockerfile" => Some(("dockerfile", "docker-langserver", vec!["--stdio"])),
        "cmake" => Some(("cmake", "cmake-language-server", vec![])),
        "proto" => Some(("proto", "protols", vec![])),
        "graphql" | "gql" => Some((
            "graphql",
            "graphql-language-service-cli",
            vec!["server", "--method", "stream"],
        )),
        "prisma" => Some(("prisma", "prisma-language-server", vec!["--stdio"])),
        "wgsl" => Some(("wgsl", "wgsl-analyzer", vec![])),
        _ => None,
    }
}

// ============================================================================
// LspClient
// ============================================================================

type ServerMap = HashMap<String, Arc<ServerEntry>>;

/// Per-document state tracked while a file is open in a language server.
struct OpenDoc {
    /// Last content sent to the server (used to decide didOpen vs didChange).
    content: String,
    /// Language key (server map key) this document was opened against, so an
    /// eviction can route `textDocument/didClose` to the right server.
    lang_key: String,
}

/// Bounded LRU set of open documents shared across all language servers.
///
/// Keeps at most `MAX_OPEN_FILES` documents. Recency is tracked in `order`
/// (front = least-recently-used, back = most-recently-used). Eviction is
/// reported back to the caller, which performs the async `didClose` outside
/// of any lock.
#[derive(Default)]
struct OpenFiles {
    docs: HashMap<String, OpenDoc>,
    order: VecDeque<String>,
}

impl OpenFiles {
    /// Move `uri` to the most-recently-used position.
    fn touch(&mut self, uri: &str) {
        if let Some(pos) = self.order.iter().position(|u| u == uri) {
            self.order.remove(pos);
        }
        self.order.push_back(uri.to_string());
    }

    /// Return the last content sent for `uri`, marking it most-recently-used.
    fn get(&mut self, uri: &str) -> Option<String> {
        let content = self.docs.get(uri).map(|d| d.content.clone())?;
        self.touch(uri);
        Some(content)
    }

    /// Record that `uri` was opened/updated against `lang_key` with `content`.
    /// If this pushes the set past the cap, the least-recently-used document
    /// (other than `uri`) is removed and returned as `(uri, lang_key)` so the
    /// caller can send `textDocument/didClose` to its server.
    fn insert(&mut self, uri: String, content: String, lang_key: String) -> Option<(String, String)> {
        self.docs.insert(uri.clone(), OpenDoc { content, lang_key });
        self.touch(&uri);

        if self.docs.len() <= MAX_OPEN_FILES {
            return None;
        }
        // Evict the least-recently-used entry from the front of the queue.
        let victim_uri = self.order.pop_front()?;
        let victim = self.docs.remove(&victim_uri)?;
        Some((victim_uri, victim.lang_key))
    }

    /// Forget every document opened against `lang_key` (e.g. when its server is
    /// shut down), so they are re-opened against a fresh server on next use.
    fn purge_lang(&mut self, lang_key: &str) {
        let docs = &mut self.docs;
        docs.retain(|_, d| d.lang_key != lang_key);
        self.order.retain(|u| docs.contains_key(u));
    }
}

pub struct LspClient {
    servers: Arc<RwLock<ServerMap>>,
    open_files: Arc<RwLock<OpenFiles>>,
}

impl LspClient {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            open_files: Arc::new(RwLock::new(OpenFiles::default())),
        }
    }

    /// Get or start a language server. Holds write lock across check-spawn-insert
    /// to prevent duplicate server processes.
    async fn ensure_server(&self, file_path: &str) -> Result<Arc<ServerEntry>> {
        let ext = Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let (lang_id, command, args) = language_config(ext)
            .ok_or_else(|| anyhow!("No language server configured for .{ext} files"))?;

        let lang_key = lang_id.to_string();

        // Check under read lock first
        if let Some(entry) = self.servers.read().await.get(&lang_key) {
            return Ok(entry.clone());
        }

        // Take write lock for the duration of spawn+init to prevent races
        let mut servers = self.servers.write().await;
        // Double-check: another task may have inserted while we waited for the write lock
        if let Some(entry) = servers.get(&lang_key) {
            return Ok(entry.clone());
        }

        let file_dir = Path::new(file_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".");
        let root_uri = format!(
            "file:///{}",
            file_dir.replace('\\', "/").trim_end_matches('/')
        );

        // Spawn via prepare_command so .cmd/.bat wrappers go through cmd /C
        // (same path as loom_mcp — without this, CreateProcess fails on .cmd).
        let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let (program, cmd_args) = prepare_command(command, &args_owned);
        let mut cmd = Command::new(&program);
        cmd.args(&cmd_args);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        let mut process = cmd.spawn().with_context(|| {
            format!("Failed to spawn '{command}' — the language server binary wasn't found on PATH. Try reinstalling it.")
        })?;

        // Drain stderr into a ring buffer so we can surface the last N lines
        // when a request fails (language server crashed on startup, etc.)
        let stderr_buf = Arc::new(StdMutex::new(VecDeque::with_capacity(50)));
        if let Some(stderr) = process.stderr.take() {
            let buf = stderr_buf.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let trimmed = line.trim_end().to_string();
                            if !trimmed.is_empty() {
                                tracing::warn!(target: "lsp_stderr", "{}", trimmed);
                                if let Ok(mut g) = buf.lock() {
                                    if g.len() >= 50 {
                                        g.pop_front();
                                    }
                                    g.push_back(trimmed);
                                }
                            }
                        }
                    }
                }
            });
        }

        let stdin = BufWriter::new(
            process
                .stdin
                .take()
                .ok_or_else(|| anyhow!("stdin unavailable"))?,
        );
        let stdout = BufReader::new(
            process
                .stdout
                .take()
                .ok_or_else(|| anyhow!("stdout unavailable"))?,
        );
        let conn = Box::new(LspConnection::new(process));

        let entry = Arc::new(ServerEntry {
            conn,
            stdin: tokio::sync::Mutex::new(stdin),
            stdout: tokio::sync::Mutex::new(stdout),
            language_id: lang_id.to_string(),
            doc_version: AtomicU64::new(1),
            diagnostics: Arc::new(StdMutex::new(HashMap::new())),
            stderr_buf,
        });

        let _result = entry
            .request(
                "initialize",
                &serde_json::json!({
                    "processId": std::process::id(),
                    "rootUri": root_uri,
                    "workspaceFolders": [{"uri": root_uri, "name": "workspace"}],
                    "capabilities": {
                        "textDocument": {
                            "diagnostic": { "dynamicRegistration": true },
                            "completion": { "completionItem": { "snippetSupport": false } },
                            "hover": { "contentFormat": ["markdown", "plaintext"] },
                            "definition": { "linkSupport": true },
                            "references": {},
                            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                        }
                    },
                    "clientInfo": { "name": "openLoom", "version": "0.2.0" }
                }),
            )
            .await?;

        entry.notify("initialized", &serde_json::json!({})).await?;
        tracing::info!(lang=%lang_id, command=%command, "LSP connected");

        servers.insert(lang_key.clone(), entry.clone());
        Ok(entry)
    }

    /// Open a document in the language server, sending didOpen or didChange as needed.
    async fn ensure_open(&self, file_path: &str, content: &str) -> Result<Arc<ServerEntry>> {
        let entry = self.ensure_server(file_path).await?;
        let uri = file_uri(file_path);

        // Brief lock: read prior content (also refreshes recency on a hit).
        let old_content = self.open_files.write().await.get(&uri);
        let needs_open = old_content.is_none();
        let needs_change = old_content.as_deref() != Some(content);

        if needs_open {
            let version = entry
                .doc_version
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            entry
                .notify(
                    "textDocument/didOpen",
                    &serde_json::json!({
                        "textDocument": {
                            "uri": uri,
                            "languageId": entry.language_id,
                            "version": version,
                            "text": content,
                        }
                    }),
                )
                .await?;
        } else if needs_change {
            let version = entry
                .doc_version
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            entry
                .notify(
                    "textDocument/didChange",
                    &serde_json::json!({
                        "textDocument": { "uri": uri, "version": version },
                        "contentChanges": [{ "text": content }]
                    }),
                )
                .await?;
        }

        if needs_open || needs_change {
            // Brief lock: record the new content and learn whether the LRU cap
            // forced an eviction. The actual `didClose` happens below, outside
            // the lock, so we never await while holding it.
            let evicted = self.open_files.write().await.insert(
                uri,
                content.to_string(),
                entry.language_id.clone(),
            );

            if let Some((evicted_uri, evicted_lang)) = evicted {
                self.close_document(&evicted_uri, &evicted_lang).await;
            }
        }

        Ok(entry)
    }

    /// Send `textDocument/didClose` for `uri` to the server identified by
    /// `lang_key`, so an evicted document is released server-side. Best-effort:
    /// failures (e.g. the server already gone) are logged and ignored, since the
    /// document is re-opened on next use anyway.
    async fn close_document(&self, uri: &str, lang_key: &str) {
        let entry = self.servers.read().await.get(lang_key).cloned();
        if let Some(entry) = entry {
            let res = entry
                .notify(
                    "textDocument/didClose",
                    &serde_json::json!({
                        "textDocument": { "uri": uri }
                    }),
                )
                .await;
            if let Err(e) = res {
                tracing::debug!(target: "lsp", %uri, lang = %lang_key, error = %e, "LSP: didClose on eviction failed");
            }
        }
    }

    // === Public API ===

    pub async fn diagnostics(&self, file_path: &str) -> Result<Value> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Cannot read file: {file_path}"))?;
        let entry = self.ensure_open(file_path, &content).await?;
        let uri = file_uri(file_path);

        // Nudge servers that support the pull model. This also drives the
        // request loop, which drains any pending `publishDiagnostics`
        // notifications into the store along the way. If the pull request
        // returns results directly, prefer those; otherwise fall back to the
        // authoritative stored push diagnostics. Servers that don't support
        // the pull method (e.g. rust-analyzer) error here 鈥?that's expected,
        // so we ignore the error and rely on the push store.
        let pulled = entry
            .request(
                "textDocument/diagnostic",
                &serde_json::json!({
                    "textDocument": { "uri": uri }
                }),
            )
            .await
            .ok()
            .and_then(|result| result.get("items").cloned());

        if let Some(Value::Array(items)) = pulled
            && !items.is_empty()
        {
            return Ok(Value::Array(items));
        }

        // Authoritative source: stored push diagnostics for this URI.
        let stored = match entry.diagnostics.lock() {
            Ok(store) => store.get(&uri).cloned(),
            Err(_) => None,
        };
        Ok(Value::Array(stored.unwrap_or_default()))
    }

    pub async fn completion(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Cannot read file: {file_path}"))?;
        let entry = self.ensure_open(file_path, &content).await?;
        let uri = file_uri(file_path);

        entry
            .request(
                "textDocument/completion",
                &serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character }
                }),
            )
            .await
    }

    pub async fn hover(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Cannot read file: {file_path}"))?;
        let entry = self.ensure_open(file_path, &content).await?;
        let uri = file_uri(file_path);

        entry
            .request(
                "textDocument/hover",
                &serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character }
                }),
            )
            .await
    }

    pub async fn definition(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Cannot read file: {file_path}"))?;
        let entry = self.ensure_open(file_path, &content).await?;
        let uri = file_uri(file_path);

        entry
            .request(
                "textDocument/definition",
                &serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character }
                }),
            )
            .await
    }

    pub async fn references(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
        include_declaration: bool,
    ) -> Result<Value> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Cannot read file: {file_path}"))?;
        let entry = self.ensure_open(file_path, &content).await?;
        let uri = file_uri(file_path);

        entry
            .request(
                "textDocument/references",
                &serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character },
                    "context": { "includeDeclaration": include_declaration }
                }),
            )
            .await
    }

    pub async fn document_symbols(&self, file_path: &str) -> Result<Value> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Cannot read file: {file_path}"))?;
        let entry = self.ensure_open(file_path, &content).await?;
        let uri = file_uri(file_path);

        entry
            .request(
                "textDocument/documentSymbol",
                &serde_json::json!({
                    "textDocument": { "uri": uri }
                }),
            )
            .await
    }

    pub async fn server_health(&self, language: &str) -> bool {
        self.servers.read().await.contains_key(language)
    }

    pub async fn list_servers(&self) -> Vec<String> {
        self.servers.read().await.keys().cloned().collect()
    }

    pub async fn shutdown(&self, language: &str) -> Result<()> {
        let entry = {
            let mut servers = self.servers.write().await;
            servers.remove(language)
        };
        if let Some(entry) = entry {
            let _ = entry.request("shutdown", &serde_json::json!({})).await;
            let _ = entry.notify("exit", &serde_json::json!({})).await;
            drop(entry);
            // Forget this server's open documents so they are re-opened against
            // a fresh server instance on next use.
            self.open_files.write().await.purge_lang(language);
            tracing::info!(%language, "LSP shutdown");
        }
        Ok(())
    }

    pub async fn shutdown_all(&self) -> Result<()> {
        let languages = self.list_servers().await;
        for lang in languages {
            let _ = self.shutdown(&lang).await;
        }
        Ok(())
    }

    pub fn supported_languages(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("rust", "rust-analyzer"),
            ("typescript", "typescript-language-server"),
            ("javascript", "typescript-language-server"),
            ("python", "pylsp"),
            ("go", "gopls"),
            ("c", "clangd"),
            ("cpp", "clangd"),
            ("java", "jdtls"),
            ("csharp", "omnisharp"),
            ("swift", "sourcekit-lsp"),
            ("kotlin", "kotlin-language-server"),
            ("scala", "metals"),
            ("ruby", "solargraph"),
            ("lua", "lua-language-server"),
            ("zig", "zls"),
            ("haskell", "haskell-language-server-wrapper"),
            ("dart", "dart"),
            ("vue", "vue-language-server"),
            ("svelte", "svelteserver"),
            ("html", "vscode-html-language-server"),
            ("css", "vscode-css-language-server"),
            ("json", "vscode-json-language-server"),
            ("yaml", "yaml-language-server"),
            ("toml", "taplo"),
            ("markdown", "marksman"),
            ("bash", "bash-language-server"),
            ("dockerfile", "docker-langserver"),
        ]
    }

    /// Collect diagnostics from all running language servers.
    /// Returns a map of language_id -> { file_path -> diagnostic count }.
    pub async fn all_diagnostics(&self) -> HashMap<String, HashMap<String, usize>> {
        let servers = self.servers.read().await;
        let mut result = HashMap::new();
        for (lang, entry) in servers.iter() {
            let mut files = HashMap::new();
            if let Ok(store) = entry.diagnostics.lock() {
                for (uri, diags) in store.iter() {
                    // file:///C:/path -> C:/path
                    let path = uri
                        .strip_prefix("file:///")
                        .map(|s| s.replace("%3A", ":").replace("%20", " "))
                        .unwrap_or_else(|| uri.clone());
                    files.insert(path, diags.len());
                }
            }
            result.insert(lang.clone(), files);
        }
        result
    }

    pub async fn start_custom(&self, language: &str, command: &str, args: &[String]) -> Result<()> {
        if self.servers.read().await.contains_key(language) {
            return Ok(());
        }

        let mut servers = self.servers.write().await;
        if servers.contains_key(language) {
            return Ok(());
        }

        let root_uri = "file:///".to_string();

        // Spawn via prepare_command so .cmd/.bat wrappers go through cmd /C
        // (same path as loom_mcp — without this, CreateProcess fails on .cmd).
        let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let (program, cmd_args) = prepare_command(command, &args_owned);
        let mut cmd = Command::new(&program);
        cmd.args(&cmd_args);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        let mut process = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn '{command}'"))?;

        // Drain stderr into a ring buffer (same pattern as ensure_server)
        let stderr_buf = Arc::new(StdMutex::new(VecDeque::with_capacity(50)));
        if let Some(stderr) = process.stderr.take() {
            let buf = stderr_buf.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let trimmed = line.trim_end().to_string();
                            if !trimmed.is_empty() {
                                tracing::warn!(target: "lsp_stderr", "{}", trimmed);
                                if let Ok(mut g) = buf.lock() {
                                    if g.len() >= 50 {
                                        g.pop_front();
                                    }
                                    g.push_back(trimmed);
                                }
                            }
                        }
                    }
                }
            });
        }

        let stdin = BufWriter::new(
            process
                .stdin
                .take()
                .ok_or_else(|| anyhow!("stdin unavailable"))?,
        );
        let stdout = BufReader::new(
            process
                .stdout
                .take()
                .ok_or_else(|| anyhow!("stdout unavailable"))?,
        );
        let conn = Box::new(LspConnection::new(process));

        let entry = Arc::new(ServerEntry {
            conn,
            stdin: tokio::sync::Mutex::new(stdin),
            stdout: tokio::sync::Mutex::new(stdout),
            language_id: language.to_string(),
            doc_version: AtomicU64::new(1),
            diagnostics: Arc::new(StdMutex::new(HashMap::new())),
            stderr_buf,
        });

        let _result = entry
            .request(
                "initialize",
                &serde_json::json!({
                    "processId": std::process::id(),
                    "rootUri": root_uri,
                    "capabilities": {
                        "textDocument": {
                            "completion": { "completionItem": { "snippetSupport": false } },
                            "hover": { "contentFormat": ["markdown", "plaintext"] }
                        }
                    },
                    "clientInfo": { "name": "openLoom", "version": "0.2.0" }
                }),
            )
            .await?;

        entry.notify("initialized", &serde_json::json!({})).await?;
        tracing::info!(language = language, command = command, "custom LSP started");

        servers.insert(language.to_string(), entry);
        Ok(())
    }
}

impl Default for LspClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Build a valid file:// URI with proper percent-encoding.
/// Handles Windows paths, spaces, non-ASCII characters (e.g. Chinese), and URI-reserved chars.
fn file_uri(file_path: &str) -> String {
    let path = file_path.replace('\\', "/");
    let encoded: String = path
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' | ':' => c.to_string(),
            _ => {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                s.bytes().map(|b| format!("%{b:02X}")).collect::<String>()
            }
        })
        .collect();

    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn open_files_evicts_least_recently_used() {
        let mut of = OpenFiles::default();
        // Fill exactly to the cap; no eviction yet.
        for i in 0..MAX_OPEN_FILES {
            let uri = format!("file:///f{i}");
            assert!(of.insert(uri, "x".into(), "rust".into()).is_none());
        }
        assert_eq!(of.docs.len(), MAX_OPEN_FILES);

        // One more insert pushes past the cap and evicts the front (f0, the LRU).
        let evicted = of
            .insert("file:///new".into(), "x".into(), "rust".into())
            .expect("over-cap insert must evict");
        assert_eq!(evicted.0, "file:///f0");
        assert_eq!(evicted.1, "rust");
        assert_eq!(of.docs.len(), MAX_OPEN_FILES);
        assert!(!of.docs.contains_key("file:///f0"));
        assert!(of.docs.contains_key("file:///new"));
    }

    #[test]
    fn open_files_get_refreshes_recency() {
        let mut of = OpenFiles::default();
        for i in 0..MAX_OPEN_FILES {
            of.insert(format!("file:///f{i}"), "x".into(), "rust".into());
        }
        // Touch f0 so it is no longer the LRU; f1 becomes the new front.
        assert_eq!(of.get("file:///f0").as_deref(), Some("x"));

        let evicted = of
            .insert("file:///new".into(), "x".into(), "rust".into())
            .expect("over-cap insert must evict");
        assert_eq!(evicted.0, "file:///f1");
        assert!(of.docs.contains_key("file:///f0"));
    }

    #[test]
    fn open_files_insert_same_uri_updates_without_eviction() {
        let mut of = OpenFiles::default();
        for i in 0..MAX_OPEN_FILES {
            of.insert(format!("file:///f{i}"), "x".into(), "rust".into());
        }
        // Re-inserting an existing uri updates content and recency but stays at
        // the cap, so it must not evict anything.
        let evicted = of.insert("file:///f5".into(), "y".into(), "rust".into());
        assert!(evicted.is_none());
        assert_eq!(of.docs.len(), MAX_OPEN_FILES);
        assert_eq!(of.get("file:///f5").as_deref(), Some("y"));
    }

    #[test]
    fn open_files_purge_lang_drops_only_matching_server() {
        let mut of = OpenFiles::default();
        of.insert("file:///a.rs".into(), "x".into(), "rust".into());
        of.insert("file:///b.py".into(), "x".into(), "python".into());
        of.insert("file:///c.rs".into(), "x".into(), "rust".into());

        of.purge_lang("rust");

        assert!(!of.docs.contains_key("file:///a.rs"));
        assert!(!of.docs.contains_key("file:///c.rs"));
        assert!(of.docs.contains_key("file:///b.py"));
        // The recency queue is kept consistent with the docs map.
        assert_eq!(of.order.len(), of.docs.len());
        assert!(of.order.iter().all(|u| of.docs.contains_key(u)));
    }
}
