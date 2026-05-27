//! LSP (Language Server Protocol) client for openLoom v2.
//!
//! Manages language server processes via stdio JSON-RPC, supporting
//! diagnostics, completion, hover, definition, references, and document symbols.

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

const MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const LSP_TIMEOUT_SECS: u64 = 30;

// ============================================================================
// LSP Connection — single language server process
// ============================================================================

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

    async fn send_request(
        &self,
        writer: &mut BufWriter<tokio::process::ChildStdin>,
        reader: &mut BufReader<tokio::process::ChildStdout>,
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

        // Read Content-Length header
        let mut header = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            if line == "\r\n" || line == "\n" || line.is_empty() {
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
                "LSP: response body too large ({} bytes, max {})",
                content_len,
                MAX_BODY_SIZE
            ));
        }

        let mut body_bytes = vec![0u8; content_len];
        tokio::io::AsyncReadExt::read_exact(reader, &mut body_bytes).await?;
        let resp: Value = serde_json::from_slice(&body_bytes).with_context(|| {
            // Safe UTF-8 truncation at char boundary
            let byte_limit = 200.min(content_len);
            let mut end = byte_limit;
            while end > 0 && end < content_len {
                if body_bytes[end] < 128 || body_bytes[end] & 0xC0 == 0xC0 {
                    break;
                }
                end -= 1;
            }
            let preview = String::from_utf8_lossy(&body_bytes[..end]);
            format!("LSP parse error: {}", preview)
        })?;

        if let Some(err) = resp.get("error") {
            return Err(anyhow!(
                "LSP error: {}",
                err["message"].as_str().unwrap_or("unknown")
            ));
        }
        Ok(resp["result"].clone())
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
// Server entry — wraps an LspConnection with its I/O handles and metadata
// ============================================================================

struct ServerEntry {
    conn: Box<LspConnection>,
    stdin: tokio::sync::Mutex<BufWriter<tokio::process::ChildStdin>>,
    stdout: tokio::sync::Mutex<BufReader<tokio::process::ChildStdout>>,
    language_id: String,
    doc_version: AtomicU64,
}

impl ServerEntry {
    async fn request(&self, method: &str, params: &Value) -> Result<Value> {
        tokio::time::timeout(Duration::from_secs(LSP_TIMEOUT_SECS), async {
            let mut stdin = self.stdin.lock().await;
            let mut stdout = self.stdout.lock().await;
            self.conn
                .send_request(&mut stdin, &mut stdout, method, params)
                .await
        })
        .await
        .map_err(|_| {
            anyhow!(
                "LSP request '{}' timed out after {}s",
                method,
                LSP_TIMEOUT_SECS
            )
        })?
    }

    async fn notify(&self, method: &str, params: &Value) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        self.conn
            .send_notification(&mut stdin, method, params)
            .await
    }
}

// ============================================================================
// Language detection and default server commands
// ============================================================================

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

pub struct LspClient {
    servers: Arc<RwLock<ServerMap>>,
    open_files: Arc<RwLock<HashMap<String, String>>>,
}

impl LspClient {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            open_files: Arc::new(RwLock::new(HashMap::new())),
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
            .ok_or_else(|| anyhow!("No language server configured for .{} files", ext))?;

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

        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        let mut process = cmd.spawn().with_context(|| {
            format!(
                "Failed to spawn '{}'. Install it to use .{} LSP features.",
                command, ext
            )
        })?;

        // Drain stderr to prevent deadlock
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
                                tracing::debug!(target: "lsp_stderr", "{}", trimmed);
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

        let old_content = self.open_files.read().await.get(&uri).cloned();
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
            self.open_files
                .write()
                .await
                .insert(uri, content.to_string());
        }

        Ok(entry)
    }

    // === Public API ===

    pub async fn diagnostics(&self, file_path: &str) -> Result<Value> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Cannot read file: {}", file_path))?;
        let entry = self.ensure_open(file_path, &content).await?;
        let uri = file_uri(file_path);

        let result = entry
            .request(
                "textDocument/diagnostic",
                &serde_json::json!({
                    "textDocument": { "uri": uri }
                }),
            )
            .await?;

        Ok(result.get("items").cloned().unwrap_or(Value::Array(vec![])))
    }

    pub async fn completion(&self, file_path: &str, line: u32, character: u32) -> Result<Value> {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Cannot read file: {}", file_path))?;
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
            .with_context(|| format!("Cannot read file: {}", file_path))?;
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
            .with_context(|| format!("Cannot read file: {}", file_path))?;
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
            .with_context(|| format!("Cannot read file: {}", file_path))?;
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
            .with_context(|| format!("Cannot read file: {}", file_path))?;
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
                s.bytes().map(|b| format!("%{:02X}", b)).collect::<String>()
            }
        })
        .collect();

    if encoded.starts_with('/') {
        format!("file://{}", encoded)
    } else {
        format!("file:///{}", encoded)
    }
}
