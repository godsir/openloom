use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "openloom", about = "Local-first private AI assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Analyze {
        #[arg(short, long)]
        input: String,
        #[arg(short, long, default_value = "profile.json")]
        output: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("openloom=info")
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Analyze { input, output } => {
            println!("Analyzing {} → {}", input, output);
            println!("Memory pipeline not yet implemented.");
        }
    }
    Ok(())
}
