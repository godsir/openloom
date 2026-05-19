use clap::{Parser, Subcommand};
use openloom_engine::Engine;
use openloom_engine::EngineConfig;
use openloom_models::{AppConfig, ChatMessage};
use openloom_server::Server;
use std::path::PathBuf;

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
    },
    /// Interactive chat (TUI)
    Chat {
        #[arg(long)]
        config: Option<String>,
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
        Commands::Serve { port, config } => {
            let _app_config = load_config(config.as_deref());
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("openLoom");

            let engine = Engine::new(EngineConfig {
                data_dir,
                threshold: 3,
                cloud_config: None,
            })?;

            let server = Server::new(engine);
            server.serve(port).await?;
        }
        Commands::Chat { config } => {
            let _app_config = load_config(config.as_deref());
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("openLoom");

            let engine = Engine::new(EngineConfig {
                data_dir,
                threshold: 3,
                cloud_config: None,
            })?;

            let sid = engine.create_session().await?.id;
            println!("openLoom chat (type /exit to quit)");
            loop {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)?;
                let line = line.trim().to_string();
                if line == "/exit" {
                    break;
                }
                if line.is_empty() {
                    continue;
                }
                let msg = ChatMessage {
                    role: "user".into(),
                    content: line,
                    timestamp: chrono::Utc::now(),
                };
                match engine.handle_message(msg, &sid).await {
                    Ok(resp) => println!("{}", resp.response),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
        }
        Commands::Run { task, config } => {
            let _app_config = load_config(config.as_deref());
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("openLoom");

            let engine = Engine::new(EngineConfig {
                data_dir,
                threshold: 3,
                cloud_config: None,
            })?;

            let sid = engine.create_session().await?.id;
            let msg = ChatMessage {
                role: "user".into(),
                content: task,
                timestamp: chrono::Utc::now(),
            };
            let resp = engine.handle_message(msg, &sid).await?;
            println!("{}", resp.response);
        }
        Commands::Skill { action } => match action {
            SkillAction::List => {
                println!(
                    "Skills: file-manager, info-retriever, schedule-reminder, code-assistant, web-browser"
                );
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
                println!("Persona: Phase 2 will display cognition summary");
            }
            MemoryAction::Events { limit } => {
                println!("Showing last {} events (Phase 2 storage query)", limit);
            }
            MemoryAction::Cognitions { subject } => {
                let data_dir = dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("openLoom");
                let engine = Engine::new(EngineConfig {
                    data_dir,
                    threshold: 3,
                    cloud_config: None,
                })?;
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
            ConfigAction::Get { key } => {
                let path = config_path(None);
                println!("Config file: {}", path.display());
                if let Some(k) = key {
                    println!("Get: {}", k);
                }
            }
            ConfigAction::Set { key, value } => {
                println!("Set {} = {}", key, value);
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
        }
        Commands::Version => {
            println!("openLoom {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Session { action } => {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("openLoom");
            let engine = Engine::new(EngineConfig {
                data_dir,
                threshold: 3,
                cloud_config: None,
            })?;
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

        match pipeline.process(session_id, text, context) {
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
}
