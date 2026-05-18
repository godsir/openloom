use clap::{Parser, Subcommand};
use openloom_memory::aggregator::PatternAggregator;
use openloom_memory::extractor::RuleBasedExtractor;
use openloom_memory::pipeline::MemoryPipeline;
use openloom_memory::store::SqliteEventStore;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "openloom",
    about = "Local-first private AI assistant",
    long_about = "A local-first AI kernel that replaces chat logs with a cognitive graph.\n\n\
                  Phase 0 — Memory Kernel MVP: extract behavioral patterns from conversation\n\
                  text and generate cognitive profiles.\n\n\
                  Input format (pipe-delimited):  session_id|context|text\n\n\
                  Examples:\n  \
                  openloom analyze -i chat.log\n  \
                  openloom analyze -i chat.log -o profile.json -t 3\n  \
                  openloom analyze -i chat.log -t 1 -v  # low threshold = more triggers",
    after_help = "Data stored in ~/.openloom/ (or %APPDATA%/openLoom/ on Windows).\n\
                  Project: https://github.com/godsir/openloom"
)]
struct Cli {
    /// Increase log verbosity (-v for DEBUG, -vv for TRACE)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract behavioral patterns from a chat log and generate a cognitive profile.
    ///
    /// The input file uses pipe-delimited format: session_id|context|text
    /// - session_id: any identifier to group messages from the same session
    /// - context: a label for the conversation domain (e.g. trading, coding, mood)
    /// - text: the actual conversation content in Chinese or English
    ///
    /// Lines starting with '#' are treated as comments.
    ///
    /// Examples:
    ///   openloom analyze -i test_data/sample_chat.log
    ///   openloom analyze -i my_chat.log -o profile.json -t 1 -v
    #[command(after_help = "INPUT FORMAT:\n  \
        Each line: session_id|context|text\n\n  \
        Example:\n  \
        s1|trading|亏了30%我又加仓了\n  \
        s1|coding|我还是更喜欢用Rust\n  \
        s2|mood|今天真的很沮丧\n\n  \
        The threshold (-t) controls how many occurrences of the same pattern\n  \
        are needed before triggering a cognition. Default is 3. Set to 1 for\n  \
        debugging or highly sensitive detection.")]
    Analyze {
        /// Input chat log file
        #[arg(short, long, value_hint = clap::ValueHint::FilePath)]
        input: String,

        /// Output JSON profile file
        #[arg(short, long, default_value = "profile.json", value_hint = clap::ValueHint::FilePath)]
        output: String,

        /// SQLite database path for event storage
        #[arg(short, long, default_value = "memory.db", value_hint = clap::ValueHint::FilePath)]
        db: String,

        /// Pattern occurrence threshold before triggering cognition (1-100)
        #[arg(short = 't', long, default_value = "3")]
        threshold: usize,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let log_level = match cli.verbose {
        0 => "openloom=info",
        1 => "openloom=debug",
        _ => "openloom=trace",
    };
    tracing_subscriber::fmt().with_env_filter(log_level).init();

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
