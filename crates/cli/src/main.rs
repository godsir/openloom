use clap::{Parser, Subcommand};
use openloom_memory::aggregator::PatternAggregator;
use openloom_memory::extractor::RuleBasedExtractor;
use openloom_memory::pipeline::MemoryPipeline;
use openloom_memory::store::SqliteEventStore;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "openloom", about = "Local-first private AI assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze a chat log file and produce a cognitive profile.
    Analyze {
        /// Input chat log file (one line per session: session_id|context|text)
        #[arg(short, long)]
        input: String,
        /// Output JSON profile file
        #[arg(short, long, default_value = "profile.json")]
        output: String,
        /// SQLite database path for event storage
        #[arg(short, long, default_value = "memory.db")]
        db: String,
        /// Number of observations needed to trigger a cognition
        #[arg(short = 't', long, default_value = "3")]
        threshold: usize,
    },
}

fn main() -> anyhow::Result<()> {
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
        } => run_analyze(&input, &output, &db, threshold)?,
    }
    Ok(())
}

fn run_analyze(
    input_path: &str,
    output_path: &str,
    db_path: &str,
    threshold: usize,
) -> anyhow::Result<()> {
    let content = fs::read_to_string(input_path)?;

    let extractor = RuleBasedExtractor::with_default_rules();
    let aggregator = PatternAggregator::new(threshold);
    let db_file = PathBuf::from(db_path);
    let _ = fs::remove_file(&db_file);
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

    fs::write(output_path, serde_json::to_string_pretty(&profile)?)?;
    println!("\nProfile written to {}", output_path);
    println!("Total events extracted: {}", total_events);
    println!("Cognitions discovered: {}", all_cognitions.len());

    Ok(())
}
