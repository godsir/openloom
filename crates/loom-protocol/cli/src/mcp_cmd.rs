//! MCP server management commands (stub).
//! Full MCP management deferred until McpManager is wired to loom-engine.

use anyhow::Result;
use loom_cli_utils::CliConfigOverrides;
use loom_config::LoaderOverrides;

#[derive(Debug, clap::Parser)]
pub struct McpCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    pub subcommand: McpSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum McpSubcommand {
    List,
    Get { name: String },
    Add { name: String, url: String },
    Remove { name: String },
    Login { name: String },
    Logout { name: String },
}

impl McpCli {
    pub async fn run(self, _loader_overrides: LoaderOverrides) -> Result<()> {
        match self.subcommand {
            McpSubcommand::List => {
                println!("No MCP servers configured. MCP management not yet implemented in openLoom.");
            }
            _ => {
                println!("MCP management not yet implemented in openLoom. Use config.toml directly.");
            }
        }
        Ok(())
    }
}
