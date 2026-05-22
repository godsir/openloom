use clap::{Parser, Subcommand};
use openloom_engine::Engine;
use openloom_engine::EngineConfig;
use openloom_models::{AppConfig, ChatMessage};
use openloom_server::Server;
use std::path::PathBuf;
use std::sync::Arc;

mod download;
mod tui;

#[derive(Parser)]
#[command(name = "openloom", about = "Local-first private AI assistant", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze chat log offline -> cognition profile
    Analyze {
        #[arg(short, long)]
        input: String,
        #[arg(short, long, default_value = "profile.json")]
        output: String,
        #[arg(short, long, default_value = "memory.db")]
        db: String,
        #[arg(short = 't', long, default_value = "3")]
        threshold: usize,
    },
    /// Start HTTP + WebSocket server (Electron sidecar mode)
    Serve {
        #[arg(long, default_value = "0")]
        port: u16,
        #[arg(long)]
        config: Option<String>,
        /// Override the local GGUF model path
        #[arg(long)]
        model: Option<String>,
    },
    /// Interactive chat (TUI)
    Chat {
        #[arg(long)]
        config: Option<String>,
        /// Override model (e.g. "anthropic:claude-sonnet-4-20250514")
        #[arg(long, short = 'm')]
        model: Option<String>,
        /// Execute a single prompt and exit (non-interactive)
        #[arg(short = 'c')]
        command: Option<String>,
        /// Continue the most recent session, or provide a session ID
        #[arg(long = "continue", short = 'r')]
        resume: Option<Option<String>>,
        /// Skip all confirmation prompts (dangerous)
        #[arg(long)]
        dangerously_skip_permissions: bool,
    },
    /// Single task execution
    Run {
        task: String,
        #[arg(long)]
        config: Option<String>,
    },
    /// Manage skills
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// View memory / cognitions
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// View / modify config
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage sessions
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// System diagnostic
    Doctor,
    /// Download a GGUF model from ModelScope
    DownloadModel {
        /// ModelScope repo ID (default: qwen/Qwen3-1.7B-GGUF)
        #[arg(long, default_value = "qwen/Qwen3-1.7B-GGUF")]
        repo: String,
        /// File to download (default: Qwen3-1.7B-Q8_0.gguf)
        #[arg(long, default_value = "Qwen3-1.7B-Q8_0.gguf")]
        file: String,
        /// Output directory (default: platform data dir /models/)
        #[arg(long)]
        output: Option<String>,
        /// List available files in the repo instead of downloading
        #[arg(long)]
        list: bool,
        /// Overwrite existing file
        #[arg(long)]
        force: bool,
    },
    /// Version info
    Version,
}

#[derive(Subcommand)]
enum SkillAction {
    List,
    Install { path: String },
    Remove { name: String },
}

#[derive(Subcommand)]
enum MemoryAction {
    Persona,
    Events {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    Cognitions {
        #[arg(long, default_value = "USER")]
        subject: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    Get { key: Option<String> },
    Set { key: String, value: String },
    Path,
}

#[derive(Subcommand)]
enum SessionAction {
    List,
    Create,
}

fn config_path(custom: Option<&str>) -> PathBuf {
    if let Some(p) = custom {
        return PathBuf::from(p);
    }
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openLoom");
    data_dir.join("config.toml")
}

fn load_config(custom_path: Option<&str>) -> AppConfig {
    let path = config_path(custom_path);
    if !path.exists() {
        tracing::warn!(path = %path.display(), "config file not found, using defaults");
        return AppConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => match toml::from_str(&content) {
            Ok(config) => config,
            Err(e) => {
                tracing::warn!(error = %e, "config parse error, using defaults");
                AppConfig::default()
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "cannot read config, using defaults");
            AppConfig::default()
        }
    }
}

fn build_engine(
    config: Option<&str>,
    rate_limit_ms: u64,
    model_override: Option<PathBuf>,
    skip_permissions: bool,
) -> anyhow::Result<Engine> {
    let app_config = load_config(config);
    let data_dir = if app_config.storage.data_dir.as_os_str().is_empty() {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("openLoom")
    } else {
        app_config.storage.data_dir.clone()
    };
    let cloud_config = app_config
        .models
        .iter()
        .find(|m| {
            matches!(
                m.backend,
                openloom_models::ModelBackend::Anthropic
                    | openloom_models::ModelBackend::OpenAI
                    | openloom_models::ModelBackend::DeepSeek
            )
        })
        .cloned();

    // Local inference via LM Studio / Ollama (separate from cloud)
    let local_config = app_config
        .models
        .iter()
        .find(|m| m.backend.is_local_inference())
        .cloned();

    // model_override from CLI takes priority; fall back to config.toml
    let model_override = model_override.or_else(|| {
        app_config
            .models
            .iter()
            .find(|m| m.backend == openloom_models::ModelBackend::LlamaCpp)
            .and_then(|m| m.path.as_ref())
            .map(|p| data_dir.join("models").join(p))
    });

    let project_scope = std::env::current_dir()
        .map(|p| format!("project:{}", p.display()))
        .unwrap_or_else(|_| "global".into());

    Engine::new(EngineConfig {
        data_dir,
        threshold: app_config.agent.max_iterations,
        cloud_config,
        local_config,
        rate_limit_ms,
        heartbeat_interval_secs: 1800,
        heartbeat_idle_threshold_min: 120,
        model_override,
        project_scope,
        skip_permissions,
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("openloom=info")
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze {
            input,
            output,
            db,
            threshold,
        } => {
            run_analyze(&input, &output, &db, threshold)?;
        }
        Commands::Serve {
            port,
            config,
            model,
        } => {
            let model_override = model.map(PathBuf::from);
            let app_config = load_config(config.as_deref());
            let rate_limit_ms = app_config.rate_limit.min_interval_ms;
            let engine = build_engine(config.as_deref(), rate_limit_ms, model_override, false)?;
            engine.load_config_into_engine(app_config).await;
            let server = Server::new(engine, config.as_ref().map(PathBuf::from));
            let shutdown_engine = server.engine().clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!("SIGINT received, shutting down...");
                shutdown_engine.shutdown().await.ok();
                std::process::exit(0);
            });
            server.serve(port).await?;
        }
        Commands::Chat {
            config,
            model,
            command,
            resume,
            dangerously_skip_permissions,
        } => {
            let model_override = model.as_ref().map(PathBuf::from);
            let engine = Arc::new(build_engine(
                config.as_deref(),
                100,
                model_override,
                dangerously_skip_permissions,
            )?);

            if let Some(prompt) = command {
                let sid = engine.create_session().await?.id;
                let msg = ChatMessage {
                    role: "user".into(),
                    content: prompt,
                    timestamp: chrono::Utc::now(),
                };
                let resp = engine.handle_message(msg, &sid, openloom_models::Mode::Code).await?;
                println!("{}", resp.response);
            } else {
                // Resolve session: --resume (latest) / --resume=<id> / new session
                let session_id = match resume {
                    Some(Some(id)) => Some(id),
                    Some(None) => {
                        // --resume without value: pick the most recent session
                        engine
                            .list_sessions()
                            .await
                            .ok()
                            .and_then(|s| s.into_iter().last().map(|s| s.id))
                    }
                    None => None,
                };
                tui::run(engine, session_id).await?;
            }
        }
        Commands::Run { task, config } => {
            let engine = build_engine(config.as_deref(), 100, None, false)?;

            let sid = engine.create_session().await?.id;
            let msg = ChatMessage {
                role: "user".into(),
                content: task,
                timestamp: chrono::Utc::now(),
            };
            let resp = engine.handle_message(msg, &sid, openloom_models::Mode::Code).await?;
            println!("{}", resp.response);
        }
        Commands::Skill { action } => match action {
            SkillAction::List => {
                let engine = build_engine(None, 100, None, false)?;
                let skills = engine.list_skills();
                if skills.is_empty() {
                    println!("No skills registered.");
                } else {
                    for s in &skills {
                        println!(
                            "{} - {} (triggers: {:?})",
                            s.name, s.description, s.triggers
                        );
                    }
                }
            }
            SkillAction::Install { path } => {
                println!("Install skill from: {} (Phase 2 WASM)", path);
            }
            SkillAction::Remove { name } => {
                println!("Remove skill: {} (not yet implemented)", name);
            }
        },
        Commands::Memory { action } => match action {
            MemoryAction::Persona => {
                let engine = build_engine(None, 100, None, false)?;
                let summary = engine.persona_summary().await;
                if summary.is_empty() {
                    println!("No persona data yet. Interact more to build a cognition profile.");
                } else {
                    println!("{}", summary);
                }
            }
            MemoryAction::Events { limit } => {
                let engine = build_engine(None, 100, None, false)?;
                let events = engine.list_events(limit).await?;
                if events.is_empty() {
                    println!("No events recorded yet.");
                } else {
                    for e in &events {
                        println!(
                            "[{}] {}: {} (conf: {:.0}%, session: {})",
                            e.timestamp,
                            e.event_type,
                            e.action,
                            e.confidence * 100.0,
                            e.source_session.as_deref().unwrap_or("-")
                        );
                    }
                }
            }
            MemoryAction::Cognitions { subject } => {
                let engine = build_engine(None, 100, None, false)?;
                let cognitions = engine.list_cognitions(&subject, 20).await?;
                if cognitions.is_empty() {
                    println!("No cognitions for subject '{}'.", subject);
                } else {
                    for c in &cognitions {
                        println!(
                            "[{}] {} (confidence: {:.0}%, evidence: {}, v{})",
                            c.trait_name,
                            c.value,
                            c.confidence * 100.0,
                            c.evidence_count,
                            c.version
                        );
                    }
                }
            }
        },
        Commands::Config { action } => match action {
            ConfigAction::Get { key } => match key {
                Some(k) => {
                    let config = load_config(None);
                    match config.get_nested(&k) {
                        Some(v) => println!("{} = {}", k, v),
                        None => println!("Key '{}' not found", k),
                    }
                }
                None => {
                    let config = load_config(None);
                    match toml::to_string_pretty(&config) {
                        Ok(s) => println!("{}", s),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
            },
            ConfigAction::Set { key, value } => {
                let path = config_path(None);
                let mut config = if path.exists() {
                    load_config(None)
                } else {
                    AppConfig::default()
                };
                if let Err(e) = config.set_nested(&key, &value) {
                    eprintln!("Error: {}", e);
                } else {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let content = toml::to_string(&config).unwrap_or_default();
                    if let Err(e) = std::fs::write(&path, content) {
                        eprintln!("Error writing config: {}", e);
                    } else {
                        println!("{} = {}", key, value);
                    }
                }
            }
            ConfigAction::Path => {
                let path = config_path(None);
                println!("{}", path.display());
            }
        },
        Commands::Doctor => {
            println!("openLoom System Diagnostic");
            println!("=========================");
            let gpu = openloom_inference::InferenceEngine::detect_gpu();
            println!(
                "GPU: vendor={}, vram={}MB, supported={}",
                gpu.vendor, gpu.vram_mb, gpu.supported
            );
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("openLoom");
            println!("Data dir: {}", data_dir.display());
            println!("Config: {}", config_path(None).display());
            // MSVC toolchain detection for Windows users
            #[cfg(target_os = "windows")]
            {
                println!();
                println!("MSVC Toolchain Detection:");
                println!("------------------------");
                detect_msvc_toolchain(&data_dir);
            }
        }
        Commands::DownloadModel {
            repo,
            file,
            output,
            list,
            force,
        } => {
            download::run(download::DownloadOpts {
                repo,
                file,
                output: output.map(PathBuf::from),
                list,
                force,
            })
            .await?;
        }
        Commands::Version => {
            println!("openLoom {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Session { action } => {
            let engine = build_engine(None, 100, None, false)?;
            match action {
                SessionAction::List => {
                    let sessions = engine.list_sessions().await?;
                    if sessions.is_empty() {
                        println!("No sessions.");
                    } else {
                        for s in &sessions {
                            println!("{}  {}  ({} msgs)", s.id, s.created_at, s.message_count);
                        }
                    }
                }
                SessionAction::Create => {
                    let s = engine.create_session().await?;
                    println!("Created session: {}", s.id);
                }
            }
        }
    }

    Ok(())
}

// === Phase 0 analyze logic (PRESERVED from original) ===
fn run_analyze(
    input_path: &str,
    output_path: &str,
    db_path: &str,
    threshold: usize,
) -> anyhow::Result<()> {
    use openloom_memory::aggregator::PatternAggregator;
    use openloom_memory::extractor::RuleBasedExtractor;
    use openloom_memory::pipeline::MemoryPipeline;
    use openloom_memory::store::SqliteEventStore;

    let content = std::fs::read_to_string(input_path)?;

    let extractor = RuleBasedExtractor::with_default_rules();
    let aggregator = PatternAggregator::new(threshold);
    let db_file = PathBuf::from(db_path);
    let _ = std::fs::remove_file(&db_file);
    let store = SqliteEventStore::open(&db_file)?;

    let mut pipeline = MemoryPipeline::new(extractor, aggregator, store);

    let mut all_cognitions: Vec<serde_json::Value> = Vec::new();
    let mut total_events = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() < 3 {
            eprintln!("Skipping malformed line: {}", line);
            continue;
        }

        let (session_id, context, text) = (parts[0], parts[1], parts[2]);

        match pipeline.process(session_id, text, context, "global") {
            Ok(result) => {
                total_events += result.events.len();
                if let Some(cog) = result.cognition_triggered {
                    println!("COGNITION TRIGGERED: {} ({})", cog.summary, cog.trait_name);
                    all_cognitions.push(serde_json::json!({
                        "trait": cog.trait_name,
                        "action": cog.action,
                        "evidence_count": cog.evidence_count,
                        "confidence": cog.confidence,
                        "summary": cog.summary,
                    }));
                }
            }
            Err(e) => {
                eprintln!("Error processing line: {}", e);
            }
        }
    }

    let profile = serde_json::json!({
        "total_events": total_events,
        "cognitions": all_cognitions,
        "generated_at": chrono::Utc::now().to_rfc3339(),
    });

    std::fs::write(output_path, serde_json::to_string_pretty(&profile)?)?;
    println!("\nProfile written to {}", output_path);
    println!("Total events extracted: {}", total_events);
    println!("Cognitions discovered: {}", all_cognitions.len());

    Ok(())
}

#[cfg(target_os = "windows")]
fn detect_msvc_toolchain(data_dir: &std::path::Path) {
    // Find VS BuildTools installation
    let vs_base = std::path::Path::new("C:/Program Files (x86)/Microsoft Visual Studio/2022");
    let editions = ["BuildTools", "Community", "Professional", "Enterprise"];

    let mut found_msvc = None;
    let mut found_sdk = None;

    for edition in &editions {
        let vc_dir = vs_base.join(edition).join("VC/Tools/MSVC");
        if let Ok(entries) = std::fs::read_dir(&vc_dir) {
            for entry in entries.flatten() {
                let ver_dir = entry.path();
                let include = ver_dir.join("include");
                if include.exists() {
                    let ver = ver_dir.file_name().unwrap().to_string_lossy().to_string();
                    found_msvc = Some((vc_dir.join(&ver), ver));
                    break;
                }
            }
        }
        if found_msvc.is_some() {
            break;
        }
    }

    // Find Windows SDK
    let kits_base = std::path::Path::new("C:/Program Files (x86)/Windows Kits/10");
    let sdk_include = kits_base.join("include");
    if let Ok(entries) = std::fs::read_dir(&sdk_include) {
        for entry in entries.flatten() {
            let ver_dir = entry.path();
            let ucrt = ver_dir.join("ucrt");
            if ucrt.exists() {
                found_sdk = Some(ver_dir.file_name().unwrap().to_string_lossy().to_string());
                break;
            }
        }
    }

    // Find LLVM / libclang
    let llvm_candidates = [
        "C:/Program Files/LLVM/bin",
        "C:/Program Files (x86)/LLVM/bin",
    ];
    let llvm_path = llvm_candidates
        .iter()
        .find(|p| std::path::Path::new(p).join("clang.exe").exists());

    // Print results
    match (&found_msvc, &found_sdk) {
        (Some((msvc_path, _ver)), Some(sdk_ver)) => {
            println!("Found MSVC: {}", msvc_path.display());
            println!("Found Windows SDK: {}", sdk_ver);
        }
        _ => {
            println!("WARNING: Could not auto-detect MSVC/Windows SDK.");
            println!("Install Visual Studio 2022 BuildTools from:");
            println!(
                "  https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022"
            );
            return;
        }
    }

    if let Some(llvm) = llvm_path {
        println!("Found LLVM/clang: {}", llvm);
    } else {
        println!(
            "WARNING: LLVM/clang not found. Install from: https://github.com/llvm/llvm-project/releases"
        );
    }

    // Suggest .cargo/config.toml content
    let msvc = found_msvc.as_ref().unwrap();
    let sdk = found_sdk.as_ref().unwrap();

    let include_dirs = [
        msvc.0.join("include"),
        kits_base.join("include").join(sdk).join("ucrt"),
        kits_base.join("include").join(sdk).join("shared"),
        kits_base.join("include").join(sdk).join("um"),
        kits_base.join("include").join(sdk).join("winrt"),
        kits_base.join("include").join(sdk).join("cppwinrt"),
    ];
    let lib_dirs = [
        msvc.0.join("lib/x64"),
        kits_base.join("lib").join(sdk).join("ucrt/x64"),
        kits_base.join("lib").join(sdk).join("um/x64"),
    ];

    println!();
    println!("Suggested .cargo/config.toml:");
    println!("[env]");
    println!(
        "INCLUDE = \"{}\"",
        include_dirs
            .iter()
            .map(|p| p.display().to_string().replace('\\', "\\\\"))
            .collect::<Vec<_>>()
            .join(";")
    );
    println!(
        "LIB = \"{}\"",
        lib_dirs
            .iter()
            .map(|p| p.display().to_string().replace('\\', "\\\\"))
            .collect::<Vec<_>>()
            .join(";")
    );
    if let Some(llvm) = llvm_path {
        println!("LIBCLANG_PATH = \"{}\"", llvm.replace('\\', "\\\\"));
    }

    // Model status — scan for any .gguf files
    let model_dir = data_dir.join("models");
    println!();
    match std::fs::read_dir(&model_dir) {
        Ok(entries) => {
            let models: Vec<_> = entries
                .filter_map(|e| {
                    let e = e.ok()?;
                    let p = e.path();
                    if p.extension().map(|x| x == "gguf").unwrap_or(false) {
                        let meta = std::fs::metadata(&p).ok()?;
                        let name = p.file_name()?.to_str()?;
                        Some((name.to_string(), meta.len()))
                    } else {
                        None
                    }
                })
                .collect();
            if models.is_empty() {
                println!(
                    "Model not found in {}. Run: cargo run -- download-model",
                    model_dir.display()
                );
            } else {
                println!("Models found in {}:", model_dir.display());
                for (name, size) in &models {
                    let size_str = if *size >= 1_073_741_824 {
                        format!("{:.2} GB", *size as f64 / 1_073_741_824.0)
                    } else {
                        format!("{:.1} MB", *size as f64 / 1_048_576.0)
                    };
                    let tag = if name.starts_with("mmproj-") {
                        " [vision proj]"
                    } else {
                        ""
                    };
                    println!("  {}  {}{}", size_str, name, tag);
                }
            }
        }
        Err(_) => {
            println!("Model directory not found. Run: cargo run -- download-model");
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn detect_msvc_toolchain(_data_dir: &std::path::Path) {
    // No-op on non-Windows
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_serve_default() {
        let args = Cli::try_parse_from(["openloom", "serve"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_serve_with_port() {
        let args = Cli::try_parse_from(["openloom", "serve", "--port", "8080"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_analyze() {
        let args = Cli::try_parse_from(["openloom", "analyze", "--input", "test.log"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_version() {
        let args = Cli::try_parse_from(["openloom", "version"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_doctor() {
        let args = Cli::try_parse_from(["openloom", "doctor"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_download_model_default() {
        let args = Cli::try_parse_from(["openloom", "download-model"]);
        assert!(args.is_ok());
    }

    #[test]
    fn test_cli_download_model_list() {
        let args = Cli::try_parse_from(["openloom", "download-model", "--list"]);
        assert!(args.is_ok());
    }
}
