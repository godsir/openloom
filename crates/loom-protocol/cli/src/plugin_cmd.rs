//! Plugin command stubs.
//! Full plugin management deferred until loom-core-plugins API is stable.

use anyhow::Result;
use loom_cli_utils::CliConfigOverrides;

#[derive(Debug, clap::Parser)]
pub struct PluginCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    pub subcommand: PluginSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum PluginSubcommand {
    Add(AddPluginArgs),
    List,
    Marketplace(MarketplaceCli),
    Remove(RemovePluginArgs),
}

#[derive(Debug, clap::Parser)]
pub struct AddPluginArgs {
    pub plugin: String,
}

#[derive(Debug, clap::Parser)]
pub struct RemovePluginArgs {
    pub plugin: String,
}

#[derive(Debug, clap::Parser)]
pub struct MarketplaceCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    pub subcommand: MarketplaceSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum MarketplaceSubcommand {
    Add { source: String },
    List,
    Upgrade,
    Remove { name: String },
}

impl MarketplaceCli {
    pub async fn run(self) -> Result<()> {
        println!("Plugin marketplace management not yet implemented in openLoom.");
        Ok(())
    }
}

pub async fn run_plugin_add(_overrides: Vec<(String, toml::Value)>, _args: AddPluginArgs) -> Result<()> {
    println!("Plugin add not yet implemented in openLoom.");
    Ok(())
}

pub async fn run_plugin_list(_overrides: Vec<(String, toml::Value)>) -> Result<()> {
    println!("No plugins installed. Plugin management not yet implemented in openLoom.");
    Ok(())
}

pub async fn run_plugin_remove(_overrides: Vec<(String, toml::Value)>, _args: RemovePluginArgs) -> Result<()> {
    println!("Plugin remove not yet implemented in openLoom.");
    Ok(())
}
