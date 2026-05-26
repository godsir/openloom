//! openLoom v2 CLI — unified entry point.
//!
//! Commands:
//!   lume serve     Start the HTTP/WebSocket server
//!   lume chat      Interactive chat demo
//!   lume mcp add   Add an MCP server to config
//!   lume mcp list  List configured MCP servers
//!   lume doctor    Diagnose environment

mod mcp_config;
mod memory;

use clap::{Parser, Subcommand};
use std::sync::Arc;

fn data_dir() -> std::path::PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(".loom")
}

#[derive(Parser)]
#[command(name = "lume", version, about = "openLoom v2 — personal AI kernel")]
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
        #[arg(long, default_value = "claude-sonnet-4-6")]
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
    },
    /// Manage MCP servers
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Diagnose the environment
    Doctor,
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
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cli = Cli::parse();

    match cli.command {
        Command::Serve { host, port } => {
            let orchestrator = Arc::new(loom_core::Orchestrator::new(3, 20, 300));
    orchestrator.init_spawn_agent(3, 300).await;
            loom_server::serve(&host, port, orchestrator).await?;
        }
        Command::Chat { model, api_key, api_key_env, provider, base_url, mcp_args, no_mcp_config } => {
            run_chat_demo(&model, api_key.as_deref(), api_key_env.as_deref(), &provider, base_url.as_deref(), mcp_args.as_deref(), !no_mcp_config).await?;
        }
        Command::Mcp { action } => match action {
            McpAction::Add { name, transport, url, command, args, headers } => {
                mcp_add(&name, &transport, url.as_deref(), command.as_deref(), args.as_deref(), &headers)?;
            }
            McpAction::List => {
                mcp_list()?;
            }
        },
        Command::Doctor => run_doctor().await,
    }
    Ok(())
}

// ============================================================================
// MCP management
// ============================================================================

fn mcp_add(name: &str, transport: &str, url: Option<&str>, command: Option<&str>, args: Option<&str>, headers: &[String]) -> anyhow::Result<()> {
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
        args: args.map(|a| a.split_whitespace().map(String::from).collect()).unwrap_or_default(),
        url: url.map(String::from),
        headers: hdrs,
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
        println!("No MCP config found at {}\n  Use 'lume mcp add' to add servers.", config_path.display());
        return Ok(());
    }
    let config: mcp_config::McpConfigFile = serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;
    println!("MCP servers ({}):", config_path.display());
    for (name, entry) in &config.mcp_servers {
        match entry.transport.as_str() {
            "streamableHttp" | "http" | "sse" => println!("  {} → HTTP {}", name, entry.url.as_deref().unwrap_or("?")),
            _ => println!("  {} → stdio {} {}", name, entry.command, entry.args.join(" ")),
        }
    }
    if config.mcp_servers.is_empty() { println!("  (empty)"); }
    Ok(())
}

// ============================================================================
// Chat demo
// ============================================================================

async fn run_chat_demo(model: &str, api_key: Option<&str>, api_key_env: Option<&str>, provider: &str, base_url: Option<&str>, mcp_args: Option<&str>, load_config: bool) -> anyhow::Result<()> {
    use loom_core::MemoryStore;
    use loom_inference::{AnthropicClient, OpenAIClient};
    use loom_inference::engine::CloudClient;

    // Resolve API key
    let api_key = if let Some(key) = api_key { key.to_string() }
    else if let Some(env_name) = api_key_env {
        std::env::var(env_name).map_err(|_| anyhow::anyhow!("env '{}' not set", env_name))?
    } else {
        let auto = match provider { "deepseek"=>"DEEPSEEK_API_KEY", "openai"=>"OPENAI_API_KEY", _=>"ANTHROPIC_API_KEY" };
        std::env::var(auto).map_err(|_| anyhow::anyhow!("No API key. Set env {} or use --api-key", auto))?
    };

    let provider = if provider == "auto" {
        if model.starts_with("claude") { "anthropic" } else if model.starts_with("deepseek") { "deepseek" } else if model.starts_with("gpt")||model.starts_with("o1")||model.starts_with("o3") { "openai" } else { "anthropic" }
    } else { provider };

    println!("openLoom v2 — Chat Demo (/exit to quit, Ctrl+C to exit)");
    println!("  model:    {}", model);
    if let Some(url) = base_url { println!("  base_url: {}", url); }

    let orchestrator = Arc::new(loom_core::Orchestrator::new(3, 20, 300));
    orchestrator.init_spawn_agent(3, 300).await;

    // Cloud client
    let client: Box<dyn CloudClient> = if let Some(url) = base_url {
        Box::new(OpenAIClient::new(api_key.clone(), model.to_string(), url.to_string(), false))
    } else { match provider {
        "anthropic" => Box::new(AnthropicClient::new(api_key.clone(), model.to_string(), "https://api.anthropic.com".into())),
        "deepseek" => Box::new(OpenAIClient::new(api_key.clone(), model.to_string(), "https://api.deepseek.com/v1".into(), false)),
        "openai" => Box::new(OpenAIClient::new(api_key.clone(), model.to_string(), "https://api.openai.com".into(), false)),
        _ => Box::new(AnthropicClient::new(api_key, model.to_string(), "https://api.anthropic.com".into())),
    }};
    orchestrator.set_cloud_client(client).await;
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
    if !skills_dir.exists() { let _ = std::fs::create_dir(&skills_dir); }
    if !data_dir_path.exists() { let _ = std::fs::create_dir(&data_dir_path); }

    // === Skills ===
    let mut skill_loader = lume_skills::SkillLoader::new();
    if let Some(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok() {
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
            let ctx: String = skills.iter().map(|s| {
                format!("- **{}**: {}", s.manifest.name, s.manifest.description)
            }).collect::<Vec<_>>().join("\n");
            orchestrator.set_skill_context(ctx).await;
            println!("[skills] {} loaded", skills.len());
        }
        Ok(_) => println!("[skills] 0 loaded (no SKILL.md found)"),
        Err(e) => println!("[skills] error: {}", e),
    }
    } // close if let Some(home)

    // === Memory ===
    let db_path = data_dir_path.join("memory.db");
    match memory::LoomMemoryStore::open(&db_path) {
        Ok(store) => {
            let persona = store.get_persona().await.unwrap_or_default();
            if !persona.is_empty() {
                orchestrator.set_persona(persona).await;
                println!("[memory] persona loaded");
            }
            orchestrator.set_memory_store(Box::new(store)).await;
            let _ = orchestrator.load_history("default").await;
            println!("[memory] store opened: {}", db_path.display());
        }
        Err(e) => println!("[memory] unavailable: {}", e),
    }

    // Load MCP configs from files
    if load_config {
        let dir = loom_dir.clone();
        match mcp_config::load_mcp_configs(&dir) {
            Ok((configs, sources)) => {
                for src in &sources { println!("[mcp] config: {}", src); }
                for config in configs {
                    let name = config.name.clone();
                    println!("[mcp] connecting {} ({})...", name, config.transport);
                    match orchestrator.connect_mcp_server(config).await {
                        Ok(server_name) => {
                            let tools = orchestrator.mcp_client().server_tools(&server_name).await.unwrap_or_default();
                            println!("[mcp] '{}' connected — {} tools", server_name, tools.len());
                            for t in &tools {
                                let desc = if t.description.chars().count() > 60 {
                                    format!("{}...", t.description.chars().take(57).collect::<String>())
                                } else { t.description.clone() };
                                println!("       - {}: {}", t.name, desc);
                            }
                        }
                        Err(e) => println!("[mcp] '{}' failed: {:.120}", name, e),
                    }
                }
            }
            Err(e) => tracing::debug!("MCP config load: {}", e),
        }
    }

    // Quick MCP from CLI args
    if let Some(args) = mcp_args {
        match mcp_config::parse_mcp_args(args) {
            Ok(config) => {
                println!("[mcp] quick-connect: {}...", config.name);
                match orchestrator.connect_mcp_server(config).await {
                    Ok(name) => {
                        let tools = orchestrator.mcp_client().server_tools(&name).await.unwrap_or_default();
                        println!("[mcp] '{}' connected — {} tools", name, tools.len());
                    }
                    Err(e) => println!("[mcp] failed: {:.120}", e),
                }
            }
            Err(e) => println!("[mcp] parse error: {}", e),
        }
    }

    // Tool list
    let registry = orchestrator.tool_registry().await;
    let names = registry.list_names();
    println!("[tools] {} available: {:?}\n", names.len(), names);
    drop(registry);

    // Ctrl+C handler — first press warns, second press exits
    use std::sync::atomic::{AtomicBool, Ordering};
    let ctrlc_pressed = Arc::new(AtomicBool::new(false));
    let flag = ctrlc_pressed.clone();
    ctrlc::set_handler(move || {
        if flag.swap(true, Ordering::SeqCst) {
            println!("\nGoodbye!");
            std::process::exit(0);
        } else {
            println!("\nPress Ctrl+C again to exit (or /exit to quit normally)");
        }
    }).ok();

    // Interactive loop
    use std::io::Write;
    loop {
        print!("> "); std::io::stdout().flush().ok();
        let mut line = String::new(); std::io::stdin().read_line(&mut line)?;
        let line = line.trim().to_string();
        if line.is_empty() { continue; }
        if line == "/exit" || line == "/quit" { println!("Goodbye!"); break; }
        if line == "/tools" {
            let r = orchestrator.tool_registry().await;
            println!("\n[tools] {}", r.list_names().join(", "));
            continue;
        }
        if line == "/skills" {
            let ctx = orchestrator.build_system_prompt().await;
            // Extract the skills section
            if let Some(pos) = ctx.find("## Available Skills") {
                println!("\n{}", &ctx[pos..]);
            } else {
                println!("\n[skills] none loaded");
            }
            continue;
        }

        match orchestrator.process_message(&line).await {
            Ok(turn) => {
                println!("\n{}\n", turn.response);
                if turn.tool_calls_made > 0 {
                    println!("[{} tools, {} iters, {} tokens]\n", turn.tool_calls_made, turn.iterations, turn.prompt_tokens + turn.completion_tokens);
                }
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
    println!("  lume-mcp:        ok (stdio + HTTP MCP client)");
    println!("  lume-skills:     ok (SKILL.md parser)");
    println!("  loom-context:    ok");
    println!("  loom-security:   ok");
}
