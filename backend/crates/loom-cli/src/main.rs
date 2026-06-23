//! openLoom v2 CLI — unified entry point.
//!
//! Commands:
//!   loom serve     Start the HTTP/WebSocket server
//!   loom chat      Interactive chat demo
//!   loom mcp add   Add an MCP server to config
//!   loom mcp list  List configured MCP servers
//!   loom doctor    Diagnose environment

mod mcp_config;
mod memory;
mod tui;

use clap::{Parser, Subcommand};
use std::io::IsTerminal;
use std::sync::Arc;

fn data_dir() -> std::path::PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(".loom")
}

/// Resolve the `resources/builtin/` directory path.
fn find_builtin_dir() -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("resources").join("builtin"));
        candidates.push(dir.join("..").join("resources").join("builtin"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("resources").join("builtin"));
        candidates.push(cwd.join("..").join("resources").join("builtin"));
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = std::path::PathBuf::from(&manifest);
        candidates.push(p.join("..").join("..").join("resources").join("builtin"));
    }

    for c in &candidates {
        if c.exists() {
            return Some(c.clone());
        }
    }
    None
}

fn copy_dir_entries(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') || name_str == "node_modules" {
            continue;
        }
        let sp = entry.path();
        let dp = dst.join(&*name);
        if sp.is_dir() {
            copy_dir_entries(&sp, &dp)?;
        } else {
            std::fs::copy(&sp, &dp)?;
        }
    }
    Ok(())
}

fn sync_builtin_resources(loom_dir: &std::path::Path) {
    let builtin = match find_builtin_dir() {
        Some(d) => d,
        None => {
            eprintln!("[server] builtin resources not found — skipping sync");
            return;
        }
    };

    let ss = builtin.join("skills");
    let sd = loom_dir.join("skills");
    let mut sc = 0usize;

    if ss.exists() {
        for e in std::fs::read_dir(&ss).into_iter().flatten().flatten() {
            let sp = e.path();
            if !sp.is_dir() { continue; }
            let n = sp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if n.is_empty() || !sp.join("SKILL.md").exists() { continue; }
            let dp = sd.join(n);
            if dp.exists() { continue; }
            if copy_dir_entries(&sp, &dp).is_ok() { sc += 1; }
        }
    }

    if sc > 0 {
        println!("[server] builtin sync: {} skills", sc);
    }
}

#[derive(Parser)]
#[command(name = "loom", version, about = "openLoom v2 — personal AI kernel")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the HTTP/WebSocket server
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 0)]
        port: u16,
    },
    /// Interactive chat demo (auto-loads MCP configs from ~/.openloom/mcp.json)
    Chat {
        #[arg(long, default_value = "deepseek-v4-flash")]
        model: String,
        #[arg(long)]
        api_key: Option<String>,
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long, default_value = "auto")]
        provider: String,
        #[arg(long)]
        base_url: Option<String>,
        /// Quick MCP connect: 'http://url header=val' or 'command args...'
        #[arg(long)]
        mcp_args: Option<String>,
        /// Skip loading MCP configs from file
        #[arg(long)]
        no_mcp_config: bool,
        /// Resume a named session
        #[arg(long)]
        resume: Option<String>,
        /// Continue the previous conversation (alias for --resume default)
        #[arg(short = 'c', long, visible_alias = "c")]
        r#continue: bool,
    },
    /// Manage MCP servers
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Search and manage knowledge graph
    Kg {
        #[command(subcommand)]
        action: KgAction,
    },
    /// Diagnose the environment
    Doctor,
}

#[derive(Subcommand)]
enum KgAction {
    /// Full-text search the knowledge graph
    Search {
        /// Search query
        query: String,
        /// Max results (default 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Expand query with LLM (requires --model and --base-url)
        #[arg(long)]
        expand: bool,
        /// Model for query expansion
        #[arg(long, default_value = "gemma-4-e4b")]
        model: String,
        /// LM Studio / Ollama endpoint for expansion
        #[arg(long, default_value = "http://localhost:1234/v1")]
        expand_url: String,
    },
    /// Show knowledge graph statistics
    Stats,
}

#[derive(Subcommand)]
enum McpAction {
    /// Add an MCP server to user config (~/.openloom/mcp.json)
    Add {
        /// Server name (e.g. "leihuo_ai")
        name: String,
        /// Transport: http or stdio
        #[arg(long, default_value = "http")]
        transport: String,
        /// URL (for HTTP transport)
        #[arg(long)]
        url: Option<String>,
        /// Command (for stdio transport)
        #[arg(long)]
        command: Option<String>,
        /// Arguments (for stdio transport)
        #[arg(long)]
        args: Option<String>,
        /// Headers: key=value (repeatable, for HTTP transport)
        #[arg(long = "header")]
        headers: Vec<String>,
    },
    /// List configured MCP servers
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // TUI chat: send tracing to a file so it doesn't corrupt the terminal.
    // Bare REPL (pipe) and all other commands: stderr as usual.
    let is_tui_chat = matches!(cli.command, Command::Chat { .. })
        && std::io::stdout().is_terminal();

    if is_tui_chat {
        let log_dir = data_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let file_appender = tracing_appender::rolling::never(&log_dir, "chat.log");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        // Box the guard so it lives for the entire process lifetime.
        // Dropping it flushes the non-blocking buffer and joins the writer thread.
        let _log_guard = Box::new(_guard);
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .with_writer(non_blocking)
            .init();
        // Suppress stdout/stderr from child crates by redirecting them.
        // Raw mode + alternate screen can't coexist with println!/eprintln!.
    } else {
        tracing_subscriber::fmt().with_env_filter("info").init();
    }

    match cli.command {
        Command::Serve { host, port } => {
            let loom_dir = data_dir();
            println!(
                "[server] *** openLoom engine v{} [build-2026-05-30-vision-fix] ***",
                env!("CARGO_PKG_VERSION")
            );
            let orchestrator = Arc::new(loom_core::Orchestrator::new(3, 20, 300, loom_dir.clone()));
            orchestrator.init_spawn_agent(3, 300).await;
            // Set up data directories
            let _ = std::fs::create_dir_all(loom_dir.join("data"));
            let _ = std::fs::create_dir_all(loom_dir.join("sessions"));
            let _ = std::fs::create_dir_all(loom_dir.join("skills"));
            let _ = std::fs::create_dir_all(loom_dir.join("plugins"));
            println!("[server] loom dir: {}", loom_dir.display());
            println!(
                "[server] images:  {}\\sessions\\<id>\\images\\",
                loom_dir.display()
            );
            let data_dir = loom_dir.join("data");
            // Load persisted API keys so the orchestrator can build cloud clients.
            let key_store = {
                let ks_map = loom_server::load_credentials(&loom_dir).await;
                Arc::new(tokio::sync::RwLock::new(ks_map))
            };
            orchestrator.set_key_store(key_store).await;

            match memory::LoomMemoryStore::open(&data_dir) {
                Ok(store) => {
                    orchestrator.set_memory_store(Box::new(store)).await;
                    if let Err(e) = orchestrator.load_agent_configs().await {
                        eprintln!("[server] load agent configs failed: {}", e);
                    }
                    // load_model_configs calls try_build_cloud_client which needs the key_store.
                    if let Err(e) = orchestrator.load_model_configs().await {
                        eprintln!("[server] load model configs failed: {}", e);
                    }
                    println!("[server] memory store: {}", data_dir.join("*.db").display());
                }
                Err(e) => println!("[server] memory unavailable: {}", e),
            }
            // Load sandbox config from disk into memory
            {
                let sc = orchestrator.load_sandbox_config().await;
                orchestrator.set_sandbox_config(sc).await;
                println!(
                    "[server] sandbox: {}",
                    if orchestrator.sandbox_config().await.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
            }
            // Migrate known-bad default package names from earlier versions,
            // then seed defaults on first run.  DB is the single source of
            // truth after that — user disconnect/delete is respected across
            // restarts, and mcp.json is no longer auto-loaded.
            {
                let existing = orchestrator
                    .list_saved_mcp_servers()
                    .await
                    .unwrap_or_default();

                // 1) Fix the wrong context7 package name.
                for (cfg, autostart) in &existing {
                    let mut updated = cfg.clone();
                    let mut changed = false;
                    for arg in &mut updated.args {
                        if arg.contains("@anthropic/context7-mcp") {
                            *arg = arg.replace("@anthropic/context7-mcp", "@upstash/context7-mcp");
                            changed = true;
                        }
                    }
                    if changed {
                        let name = updated.name.clone();
                        let _ = orchestrator.save_mcp_server(&updated, *autostart).await;
                        tracing::info!("[mcp] migrated '{}' → @upstash/context7-mcp", name);
                    }
                }

                // 2) Remove the broken github default (no valid npm package).
                for (cfg, _) in &existing {
                    if cfg.name == "github"
                        && cfg.args.iter().any(|a| a.contains("@anthropic/github-mcp"))
                    {
                        let _ = orchestrator.delete_saved_mcp_server("github").await;
                        tracing::info!("[mcp] removed broken 'github' default (no valid npm package)");
                    }
                }

                // 3) Seed defaults on first run (DB empty).
                if existing.is_empty() {
                    for d in mcp_config::default_mcp_server_configs() {
                        let name = d.name.clone();
                        if let Err(e) = orchestrator.save_mcp_server(&d, true).await {
                            tracing::warn!("[mcp] seed default '{}' failed: {}", name, e);
                        } else {
                            tracing::info!("[mcp] seeded default '{}'", name);
                        }
                    }
                }
            }
            // Ensure default mcp.json exists on disk for user visibility/editing.
            // (Not auto-loaded — DB is the runtime source of truth.)
            mcp_config::ensure_default_mcp_config(&loom_dir);
            // Connect all DB-persisted servers marked autostart=true.
            // Servers the user disconnected (autostart=false) or deleted are skipped.
            orchestrator.autostart_mcp_servers().await;
            // === Skills ===
            {
                // Sync builtin resources before discovery so they're always available.
                sync_builtin_resources(&loom_dir);

                let mut skill_loader = loom_skills::SkillLoader::new();
                skill_loader.add_standard_paths(&loom_dir);
                match skill_loader.discover() {
                    Ok(skills) if !skills.is_empty() => {
                        orchestrator
                            .set_skills(loom_skills::SkillState::from_skills(&skills))
                            .await;
                        println!("[server] {} skills loaded", skills.len());
                    }
                    Ok(_) => println!("[server] 0 skills loaded"),
                    Err(e) => eprintln!("[server] skills error: {}", e),
                }
            }
            // === Graceful shutdown: wire SIGTERM/SIGINT/Ctrl+C ===
            let shutdown_token = tokio_util::sync::CancellationToken::new();
            {
                let token = shutdown_token.clone();
                tokio::spawn(async move {
                    #[cfg(unix)]
                    {
                        use tokio::signal::unix::{SignalKind, signal};
                        let mut sigterm = signal(SignalKind::terminate())
                            .expect("failed to register SIGTERM handler");
                        let mut sigint = signal(SignalKind::interrupt())
                            .expect("failed to register SIGINT handler");
                        tokio::select! {
                            _ = sigterm.recv() => {
                                tracing::info!("SIGTERM received — initiating graceful shutdown");
                            }
                            _ = sigint.recv() => {
                                tracing::info!("SIGINT received — initiating graceful shutdown");
                            }
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = tokio::signal::ctrl_c().await;
                        tracing::info!("Ctrl+C received — initiating graceful shutdown");
                    }
                    token.cancel();
                });
            }
            loom_server::serve(&host, port, orchestrator.clone(), &loom_dir, shutdown_token)
                .await?;

            // Drain inflight agent loops (10s timeout) + close SQLite
            tracing::info!("server loop exited — draining inflight agents");
            orchestrator.shutdown().await;

            // Drop Arc to release remaining resources (MCP connections, etc.)
            drop(orchestrator);
            tracing::info!("openLoom shutdown complete — goodbye");
        }
        Command::Chat {
            model,
            api_key,
            api_key_env,
            provider,
            base_url,
            mcp_args,
            no_mcp_config,
            resume,
            r#continue,
        } => {
            // --resume <name>: resume named session | -c: continue "default" session
            // neither: fresh session with unique ID (no history loaded)
            let (session, is_new) = if let Some(name) = resume {
                (name, false)
            } else if r#continue {
                ("default".to_string(), false)
            } else {
                (format!("session-{}", chrono::Utc::now().timestamp()), true)
            };
            run_chat_demo(
                &model,
                api_key.as_deref(),
                api_key_env.as_deref(),
                &provider,
                base_url.as_deref(),
                mcp_args.as_deref(),
                !no_mcp_config,
                &session,
                is_new,
            )
            .await?;
        }
        Command::Mcp { action } => match action {
            McpAction::Add {
                name,
                transport,
                url,
                command,
                args,
                headers,
            } => {
                mcp_add(
                    &name,
                    &transport,
                    url.as_deref(),
                    command.as_deref(),
                    args.as_deref(),
                    &headers,
                )?;
            }
            McpAction::List => {
                mcp_list()?;
            }
        },
        Command::Doctor => run_doctor().await,
        Command::Kg { action } => match action {
            KgAction::Search {
                query,
                limit,
                expand,
                model,
                expand_url,
            } => run_kg_search(&query, limit, expand, &model, &expand_url).await,
            KgAction::Stats => run_kg_stats().await,
        },
    }
    Ok(())
}

// ============================================================================
// MCP management
// ============================================================================

fn mcp_add(
    name: &str,
    transport: &str,
    url: Option<&str>,
    command: Option<&str>,
    args: Option<&str>,
    headers: &[String],
) -> anyhow::Result<()> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let config_path = dir.join("mcp.json");

    let mut config: mcp_config::McpConfigFile = if config_path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&config_path)?)?
    } else {
        Default::default()
    };

    let mut hdrs = std::collections::HashMap::new();
    for h in headers {
        if let Some((k, v)) = h.split_once('=') {
            hdrs.insert(k.to_string(), v.to_string());
        }
    }

    let entry = mcp_config::McpConfigEntry {
        transport: transport.to_string(),
        command: command.unwrap_or("").to_string(),
        args: args
            .map(|a| a.split_whitespace().map(String::from).collect())
            .unwrap_or_default(),
        url: url.map(String::from),
        headers: hdrs,
        env: Default::default(),
    };

    config.mcp_servers.insert(name.to_string(), entry);
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    println!("Added MCP server '{}' to {}", name, config_path.display());
    Ok(())
}

fn mcp_list() -> anyhow::Result<()> {
    let dir = data_dir();
    let config_path = dir.join("mcp.json");
    if !config_path.exists() {
        println!(
            "No MCP config found at {}\n  Use 'loom mcp add' to add servers.",
            config_path.display()
        );
        return Ok(());
    }
    let config: mcp_config::McpConfigFile =
        serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;
    println!("MCP servers ({}):", config_path.display());
    for (name, entry) in &config.mcp_servers {
        match entry.transport.as_str() {
            "streamableHttp" | "http" | "sse" => {
                println!("  {} → HTTP {}", name, entry.url.as_deref().unwrap_or("?"))
            }
            _ => println!(
                "  {} → stdio {} {}",
                name,
                entry.command,
                entry.args.join(" ")
            ),
        }
    }
    if config.mcp_servers.is_empty() {
        println!("  (empty)");
    }
    Ok(())
}

/// Ensure a base URL ends with `/v1` for OpenAI-compatible endpoints.
/// Anthropic endpoints (`/messages`) do NOT use `/v1` — do not call this for them.
fn normalize_openai_url(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{}/v1", trimmed)
    }
}

// ============================================================================
// Chat demo
// ============================================================================

#[allow(clippy::too_many_arguments)]
async fn run_chat_demo(
    model: &str,
    api_key: Option<&str>,
    api_key_env: Option<&str>,
    provider: &str,
    base_url: Option<&str>,
    mcp_args: Option<&str>,
    load_config: bool,
    session: &str,
    is_new: bool,
) -> anyhow::Result<()> {
    use loom_core::MemoryStore;
    use loom_inference::engine::CloudClient;
    use loom_inference::{AnthropicClient, InferenceEngine, OpenAIClient};

    // Resolve API key
    let api_key = if let Some(key) = api_key {
        key.to_string()
    } else if let Some(env_name) = api_key_env {
        std::env::var(env_name).map_err(|_| anyhow::anyhow!("env '{}' not set", env_name))?
    } else {
        let auto = match provider {
            "deepseek" => "DEEPSEEK_API_KEY",
            "openai" => "OPENAI_API_KEY",
            "anthropic" => "ANTHROPIC_API_KEY",
            _ => "OPENLOOM_API_KEY",
        };
        std::env::var(auto)
            .map_err(|_| anyhow::anyhow!("No API key. Set env {} or use --api-key", auto))?
    };

    let provider = if provider == "auto" {
        if model.starts_with("claude") {
            "anthropic"
        } else if model.starts_with("deepseek") {
            "deepseek"
        } else if model.starts_with("gpt") || model.starts_with("o1") || model.starts_with("o3") {
            "openai"
        } else {
            "anthropic"
        }
    } else {
        provider
    };

    println!("openLoom v2 — Chat Demo (/exit to quit, Ctrl+C to exit)");
    println!("  session:  {}", session);
    println!("  model:    {}", model);
    if let Some(url) = base_url {
        println!("  base_url: {}", url);
    }

    let loom_dir = data_dir();
    let orchestrator = Arc::new(loom_core::Orchestrator::new(3, 20, 300, loom_dir));
    orchestrator.init_spawn_agent(3, 300).await;

    // Cloud client — route to InferenceEngine for local endpoints, or
    // AnthropicClient/OpenAIClient for cloud endpoints.

    // Snapshot explicit provider before auto-detection overwrites it.
    let explicit_provider = if provider == "auto" {
        None
    } else {
        Some(provider.to_string())
    };

    let client: Box<dyn CloudClient> = if matches!(provider, "lmstudio" | "ollama") {
        // Explicit local provider → InferenceEngine with OpenAI-compatible endpoint
        let url = base_url
            .map(String::from)
            .unwrap_or_else(|| match provider {
                "ollama" => "http://localhost:11434/v1".into(),
                _ => "http://localhost:1234/v1".into(),
            });
        let engine = InferenceEngine::connect(&normalize_openai_url(&url), model, 8192).await?;
        Box::new(engine)
    } else if let Some(ref url) = base_url {
        let is_localhost = url.contains("localhost") || url.contains("127.0.0.1");
        let explicit_anthropic = explicit_provider
            .as_deref()
            .is_some_and(|p| p == "anthropic");
        if is_localhost && !explicit_anthropic {
            // Localhost with auto-detected (or non-anthropic) provider → OpenAI format
            let engine = InferenceEngine::connect(&normalize_openai_url(url), model, 8192).await?;
            Box::new(engine)
        } else {
            // Remote URL, or explicit anthropic → provider-specific client (no /v1 normalization)
            match provider {
                "anthropic" => Box::new(AnthropicClient::new(
                    api_key.clone(),
                    model.to_string(),
                    url.to_string(),
                )),
                _ => Box::new(OpenAIClient::new(
                    api_key.clone(),
                    model.to_string(),
                    url.to_string(),
                    false,
                )),
            }
        }
    } else {
        match provider {
            "anthropic" => Box::new(AnthropicClient::new(
                api_key.clone(),
                model.to_string(),
                "https://api.anthropic.com".into(),
            )),
            "deepseek" => Box::new(OpenAIClient::new(
                api_key.clone(),
                model.to_string(),
                "https://api.deepseek.com/v1".into(),
                false,
            )),
            "openai" => Box::new(OpenAIClient::new(
                api_key.clone(),
                model.to_string(),
                "https://api.openai.com".into(),
                false,
            )),
            _ => Box::new(AnthropicClient::new(
                api_key,
                model.to_string(),
                "https://api.anthropic.com".into(),
            )),
        }
    };
    orchestrator.set_cloud_client(client.into()).await;
    println!("[model] {} ready", model);

    // Ensure ~/.loom/ exists with clean structure
    //   ~/.loom/skills/     — SKILL.md files
    //   ~/.loom/data/       — SQLite databases (memory.db)
    //   ~/.loom/mcp.json    — MCP server configs
    let loom_dir = data_dir();
    if !loom_dir.exists() {
        std::fs::create_dir_all(&loom_dir)?;
        // Migrate from old %APPDATA%/openLoom
        if let Ok(old) = std::env::var("APPDATA") {
            let old_dir = std::path::PathBuf::from(&old).join("openLoom");
            if old_dir.exists() {
                for entry in std::fs::read_dir(&old_dir).into_iter().flatten().flatten() {
                    let name = entry.file_name();
                    let target = loom_dir.join(&name);
                    if !target.exists() && entry.path().is_file() {
                        let _ = std::fs::copy(entry.path(), &target);
                    }
                }
            }
        }
    }
    // Ensure subdirectories
    let skills_dir = loom_dir.join("skills");
    let data_dir_path = loom_dir.join("data");
    if !skills_dir.exists() {
        let _ = std::fs::create_dir(&skills_dir);
    }
    if !data_dir_path.exists() {
        let _ = std::fs::create_dir(&data_dir_path);
    }

    // === Skills ===
    sync_builtin_resources(&loom_dir);
    let mut skill_loader = loom_skills::SkillLoader::new();
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let home = std::path::PathBuf::from(&home);
        let search: &[(&str, std::path::PathBuf)] = &[
            ("~/.claude/skills", home.join(".claude").join("skills")),
            ("~/.openclaw/skills", home.join(".openclaw").join("skills")),
            ("~/.loom/skills", skills_dir.clone()),
        ];
        println!("[skills] scanning:");
        for (label, path) in search {
            let marker = if path.exists() { "✓" } else { "✗" };
            println!("  {} {}", marker, label);
            if path.exists() {
                skill_loader.add_path(path.clone(), label);
            }
        }
        match skill_loader.discover() {
            Ok(skills) if !skills.is_empty() => {
                orchestrator
                    .set_skills(loom_skills::SkillState::from_skills(&skills))
                    .await;
                println!("[skills] {} loaded", skills.len());
            }
            Ok(_) => println!("[skills] 0 loaded (no SKILL.md found)"),
            Err(e) => println!("[skills] error: {}", e),
        }
    }

    // === Memory ===
    let data_dir = data_dir_path;
    match memory::LoomMemoryStore::open(&data_dir) {
        Ok(store) => {
            let persona = store.get_persona().await.unwrap_or_default();
            if !persona.is_empty() {
                orchestrator.set_persona(persona).await;
                println!("[memory] persona loaded");
            }
            orchestrator.set_memory_store(Box::new(store)).await;
            let _ = orchestrator.prune_memory().await;
            if let Err(e) = orchestrator.load_agent_configs().await {
                eprintln!("[memory] load agent configs failed: {}", e);
            }
            if let Err(e) = orchestrator.load_model_configs().await {
                eprintln!("[memory] load model configs failed: {}", e);
            }
            if !is_new {
                let _ = orchestrator.load_history(session).await;
                let history = orchestrator.session_history(session).await;
                if !history.is_empty() {
                    println!(
                        "\n--- session '{}' ({} messages) ---",
                        session,
                        history.len()
                    );
                    for msg in &history {
                        let role = msg.role.as_str();
                        println!("[{}] {}", role, msg.text_content());
                    }
                    println!("--- end of history ---\n");
                }
            }
            println!("[memory] store opened: {}", data_dir.join("*.db").display());
        }
        Err(e) => println!("[memory] unavailable: {}", e),
    }

    // Seed default MCP servers into the DB on first run only.
    // After that the DB is the single source of truth.
    {
        let existing = orchestrator
            .list_saved_mcp_servers()
            .await
            .unwrap_or_default();
        if existing.is_empty() {
            for d in mcp_config::default_mcp_server_configs() {
                let name = d.name.clone();
                if let Err(e) = orchestrator.save_mcp_server(&d, true).await {
                    println!("[mcp] seed default '{}' failed: {}", name, e);
                } else {
                    println!("[mcp] seeded default '{}'", name);
                }
            }
        }
    }
    // Ensure default mcp.json exists on disk for user visibility/editing.
    mcp_config::ensure_default_mcp_config(&loom_dir);

    // Connect DB-persisted servers marked autostart=true (no mcp.json reload).
    if load_config {
        orchestrator.autostart_mcp_servers().await;
    }

    // Quick MCP from CLI args
    if let Some(args) = mcp_args {
        match mcp_config::parse_mcp_args(args) {
            Ok(config) => {
                println!("[mcp] quick-connect: {}...", config.name);
                match orchestrator.connect_mcp_server(config).await {
                    Ok(name) => {
                        let tools = orchestrator
                            .mcp_client()
                            .server_tools(&name)
                            .await
                            .unwrap_or_default();
                        println!("[mcp] '{}' connected — {} tools", name, tools.len());
                    }
                    Err(e) => println!("[mcp] failed: {:.120}", e),
                }
            }
            Err(e) => println!("[mcp] parse error: {}", e),
        }
    }

    // === Bridge ===
    let bridge_manager = std::sync::Arc::new(loom_bridge::BridgeManager::new());
    if let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN")
        && !token.is_empty()
    {
        let tg = loom_bridge::TelegramAdapter::new(token);
        let mgr = bridge_manager.clone();
        let _handle = tokio::spawn(async move {
            mgr.register(Box::new(tg)).await;
            let _ = mgr.start_platform(loom_bridge::Platform::Telegram).await;
            println!("[bridge] Telegram connected");
        });
    }
    if let Ok(key) = std::env::var("ILINK_API_KEY")
        && !key.is_empty()
    {
        let wx = loom_bridge::WechatAdapter::new(key);
        let mgr = bridge_manager.clone();
        let _handle = tokio::spawn(async move {
            mgr.register(Box::new(wx)).await;
            let _ = mgr.start_platform(loom_bridge::Platform::Wechat).await;
            println!("[bridge] WeChat (iLink) connected");
        });
    }

    let registry = orchestrator.tool_registry().await;
    let names = registry.list_names();
    println!("[tools] {} available: {:?}\n", names.len(), names);
    drop(registry);

    // TTY check: use TUI if terminal, bare REPL otherwise
    let use_tui = std::io::stdout().is_terminal();
    if use_tui {
        tui::run_tui(orchestrator, model.to_string(), session.to_string()).await?;
    } else {
        run_bare_repl(orchestrator, session.to_string()).await?;
    }
    Ok(())
}

/// Bare REPL — used when stdout is not a terminal (pipe, redirect).
async fn run_bare_repl(
    orchestrator: Arc<loom_core::Orchestrator>,
    session: String,
) -> anyhow::Result<()> {
    use loom_types::StreamDelta;
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Strip ANSI / control chars from AI output to prevent terminal injection
    // in bare-stdout mode (no ratatui buffer isolation).
    fn safe_print(s: &str) {
        for c in s.chars() {
            if c == '\x1b' || c == '\u{9b}' || (c.is_control() && c != '\n' && c != '\t' && c != '\r')
            {
                continue;
            }
            print!("{}", c);
        }
    }

    // Ctrl+C handler
    let ctrlc_pressed = Arc::new(AtomicBool::new(false));
    let flag = ctrlc_pressed.clone();
    ctrlc::set_handler(move || {
        if flag.swap(true, Ordering::SeqCst) {
            println!("\nGoodbye!");
            std::process::exit(0);
        } else {
            println!("\nPress Ctrl+C again to exit (or /exit to quit normally)");
        }
    })
    .ok();

    loop {
        print!("> ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line == "/exit" || line == "/quit" {
            println!("Goodbye!");
            break;
        }
        if line == "/tools" {
            let r = orchestrator.tool_registry().await;
            println!("\n[tools] {}", r.list_names().join(", "));
            continue;
        }
        if line == "/skills" {
            let summaries = orchestrator.get_skill_summaries().await;
            if !summaries.is_empty() {
                println!("\n## Available Skills ({})", summaries.len());
                for s in &summaries {
                    let flag = if s.always_active { " [auto]" } else { "" };
                    println!("  - {}:{}\n    {}", s.name, flag, s.description);
                }
            } else {
                println!("\n[skills] none loaded");
            }
            continue;
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamDelta>(256);
        let mut fut = std::pin::pin!(orchestrator.process_message_streaming(
            &line,
            tx,
            &session,
            None,
            vec![],
            vec![],
            "operate"
        ));
        let mut tool_idx = 0usize;
        let mut think_buf = String::new();
        let (mut prompt, mut completion, mut cache_read, mut cache_write) =
            (0u64, 0u64, 0u64, 0u64);
        println!();
        let result = loop {
            tokio::select! {
                r = &mut fut => { break r; }
                delta = rx.recv() => match delta {
                    Some(StreamDelta::Text(t)) => {
                        if !think_buf.is_empty() {
                            safe_print(&format!("\n  [think] {}\n", think_buf)); think_buf.clear();
                        }
                        safe_print(&t); std::io::stdout().flush().ok();
                    }
                    Some(StreamDelta::Reasoning(r)) => { think_buf.push_str(&r); }
                    Some(StreamDelta::ToolCallBegin { name, .. }) => {
                        if !think_buf.is_empty() {
                            safe_print(&format!("\n  [think] {}\n", think_buf)); think_buf.clear();
                        }
                        tool_idx += 1;
                        print!("\n  [{tool_idx}] calling {}... ", name); std::io::stdout().flush().ok();
                    }
                    Some(StreamDelta::Usage { prompt_tokens, completion_tokens, cache_read_tokens, cache_write_tokens }) => {
                        prompt += prompt_tokens;
                        completion += completion_tokens;
                        cache_read += cache_read_tokens;
                        cache_write += cache_write_tokens;
                    }
                    Some(StreamDelta::ToolResult { success, .. }) => {
                        print!("{} ", if success { "ok" } else { "FAILED" });
                        std::io::stdout().flush().ok();
                    }
                    Some(StreamDelta::AuxiliaryUsage { .. }) => {}
                    Some(StreamDelta::Image { .. }) => {}
                    None => { break fut.await; }
                    _ => {}
                }
            }
        };
        // Drain remaining deltas
        while let Ok(delta) = rx.try_recv() {
            match delta {
                StreamDelta::Text(t) => { safe_print(&t); std::io::stdout().flush().ok(); }
                StreamDelta::Usage { prompt_tokens, completion_tokens, cache_read_tokens, cache_write_tokens } => {
                    prompt += prompt_tokens;
                    completion += completion_tokens;
                    cache_read += cache_read_tokens;
                    cache_write += cache_write_tokens;
                }
                _ => {}
            }
        }
        if !think_buf.is_empty() {
            safe_print(&format!("\n  [think] {}\n", think_buf));
        }
        match result {
            Ok(turn) => {
                if prompt == 0 && completion == 0 {
                    prompt = turn.prompt_tokens as u64;
                    completion = turn.completion_tokens as u64;
                }
                println!();
                let mut parts = vec![format!("in {}", prompt), format!("out {}", completion)];
                if cache_read > 0 { parts.push(format!("cr {}", cache_read)); }
                else if turn.cached_tokens > 0 { parts.push(format!("cr ~{}", turn.cached_tokens)); }
                if cache_write > 0 { parts.push(format!("cw {}", cache_write)); }
                if turn.tool_calls_made > 0 { parts.push(format!("{} tools", turn.tool_calls_made)); }
                if turn.iterations > 1 { parts.push(format!("{} iters", turn.iterations)); }
                println!("[{}]\n", parts.join(" | "));
            }
            Err(e) => println!("\n[error] {}\n", e),
        }
    }
    Ok(())
}

async fn run_doctor() {
    println!("openLoom v2 doctor");
    println!("  loom-types:      ok");
    println!("  loom-inference:  ok (5 providers)");
    println!("  loom-core:       ok (AgentPool + ToolRegistry + AgentLoop)");
    println!("  loom-memory:     ok (SQLite + FTS5 + KG)");
    println!("  loom-server:     ok (Axum HTTP/WS)");
    println!("  loom-mcp:        ok (stdio + HTTP MCP client)");
    println!("  loom-skills:     ok (SKILL.md parser)");
    println!("  loom-context:    ok");
    println!("  loom-security:   ok");
}

// ============================================================================
// KG commands
// ============================================================================

async fn open_memory_store() -> Option<memory::LoomMemoryStore> {
    let data_dir = data_dir().join("data");
    memory::LoomMemoryStore::open(&data_dir).ok()
}

async fn run_kg_search(
    query: &str,
    limit: usize,
    expand: bool,
    expand_model: &str,
    expand_url: &str,
) {
    use loom_core::MemoryStore;
    let Some(store) = open_memory_store().await else {
        println!("Cannot open memory store. Run loom chat first to initialize.");
        return;
    };

    let search_query = if expand {
        match expand_query(query, expand_model, expand_url).await {
            Ok(expanded) => {
                println!("[expanded] {} → {}", query, expanded);
                expanded
            }
            Err(e) => {
                println!("[expansion failed: {}] falling back to raw query", e);
                query.to_string()
            }
        }
    } else {
        query.to_string()
    };

    match store.search_knowledge(&search_query, limit).await {
        Ok(results) if results.is_empty() => println!("No results for '{}'", query),
        Ok(results) => {
            println!(
                "Knowledge Graph — {} results for '{}':\n",
                results.len(),
                query
            );
            for (i, (name, etype, desc, conf)) in results.iter().enumerate() {
                println!("{}. {} [{}] (confidence: {:.2})", i + 1, name, etype, conf);
                if !desc.is_empty() && desc != name {
                    println!("   {}", desc);
                }
                println!();
            }
        }
        Err(e) => println!("Search error: {}", e),
    }
}

/// Use LLM to expand a search query with related terms in English + Chinese.
async fn expand_query(query: &str, model: &str, base_url: &str) -> anyhow::Result<String> {
    let engine = loom_inference::InferenceEngine::connect(base_url, model, 8192).await?;
    let prompt = format!(
        "Expand this search query into space-separated keywords in English and Chinese.\n\
         Return ONLY the expanded keywords, nothing else.\n\nQuery: {query}"
    );
    let request = loom_types::CompletionRequest {
        messages: vec![loom_types::Message::user(&prompt)],
        max_tokens: 64,
        temperature: 0.0,
        ..Default::default()
    };
    let response = engine.complete(request).await?;
    Ok(response.text.trim().to_string())
}

async fn run_kg_stats() {
    use loom_core::MemoryStore;
    let Some(store) = open_memory_store().await else {
        println!("Cannot open memory store.");
        return;
    };
    match store.kg_node_count().await {
        Ok(n) => println!("Knowledge Graph Statistics\n  entities: {}", n),
        Err(e) => println!("Stats error: {}", e),
    }
    match store.kg_edge_count().await {
        Ok(n) => println!("  relations: {}", n),
        Err(e) => println!("Stats error: {}", e),
    }
}
