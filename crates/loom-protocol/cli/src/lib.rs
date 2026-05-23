pub(crate) mod debug_sandbox {
    //! Stub sandbox command runners.
    //! Full sandbox implementation deferred until loom-sandboxing crate is stable.

    use crate::{LandlockCommand, SeatbeltCommand, WindowsCommand};
    use std::path::PathBuf;

    pub async fn run_command_under_seatbelt(
        _cmd: SeatbeltCommand,
        _linux_sandbox_exe: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        anyhow::bail!("sandbox (seatbelt) not yet implemented in openLoom")
    }

    pub async fn run_command_under_landlock(
        _cmd: LandlockCommand,
        _linux_sandbox_exe: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        anyhow::bail!("sandbox (landlock) not yet implemented in openLoom")
    }

    pub async fn run_command_under_windows(
        _cmd: WindowsCommand,
        _linux_sandbox_exe: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        anyhow::bail!("sandbox (windows) not yet implemented in openLoom")
    }
}
mod exit_status;

use clap::Parser;
use loom_absolute_path::AbsolutePathBuf;
use loom_cli_utils::CliConfigOverrides;
use std::path::PathBuf;

pub use debug_sandbox::run_command_under_landlock;
pub use debug_sandbox::run_command_under_seatbelt;
pub use debug_sandbox::run_command_under_windows;

// TODO: Deduplicate these shared sandbox options if we remove the explicit
// `loom sandbox <os>` platform subcommands.
#[derive(Debug, Parser)]
pub struct SeatbeltCommand {
    /// Named permissions profile to apply from the active configuration stack.
    #[arg(long = "permissions-profile", value_name = "NAME")]
    pub permissions_profile: Option<String>,

    /// Working directory used for profile resolution and command execution.
    #[arg(
        short = 'C',
        long = "cd",
        value_name = "DIR",
        requires = "permissions_profile"
    )]
    pub cwd: Option<PathBuf>,

    /// Include managed requirements while resolving an explicit permissions profile.
    #[arg(
        long = "include-managed-config",
        default_value_t = false,
        requires = "permissions_profile"
    )]
    pub include_managed_config: bool,

    /// Allow the sandboxed command to bind/connect AF_UNIX sockets rooted at this path. Relative paths are resolved against the current directory. Repeat to allow multiple paths.
    #[arg(long = "allow-unix-socket", value_parser = parse_allow_unix_socket_path)]
    pub allow_unix_sockets: Vec<AbsolutePathBuf>,

    /// While the command runs, capture macOS sandbox denials via `log stream` and print them after exit
    #[arg(long = "log-denials", default_value_t = false)]
    pub log_denials: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Full command args to run under seatbelt.
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

fn parse_allow_unix_socket_path(raw: &str) -> Result<AbsolutePathBuf, String> {
    AbsolutePathBuf::relative_to_current_dir(raw)
        .map_err(|err| format!("invalid path {raw}: {err}"))
}

#[derive(Debug, Parser)]
pub struct LandlockCommand {
    /// Named permissions profile to apply from the active configuration stack.
    #[arg(long = "permissions-profile", value_name = "NAME")]
    pub permissions_profile: Option<String>,

    /// Working directory used for profile resolution and command execution.
    #[arg(
        short = 'C',
        long = "cd",
        value_name = "DIR",
        requires = "permissions_profile"
    )]
    pub cwd: Option<PathBuf>,

    /// Include managed requirements while resolving an explicit permissions profile.
    #[arg(
        long = "include-managed-config",
        default_value_t = false,
        requires = "permissions_profile"
    )]
    pub include_managed_config: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Full command args to run under the Linux sandbox.
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct WindowsCommand {
    /// Named permissions profile to apply from the active configuration stack.
    #[arg(long = "permissions-profile", value_name = "NAME")]
    pub permissions_profile: Option<String>,

    /// Working directory used for profile resolution and command execution.
    #[arg(
        short = 'C',
        long = "cd",
        value_name = "DIR",
        requires = "permissions_profile"
    )]
    pub cwd: Option<PathBuf>,

    /// Include managed requirements while resolving an explicit permissions profile.
    #[arg(
        long = "include-managed-config",
        default_value_t = false,
        requires = "permissions_profile"
    )]
    pub include_managed_config: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Full command args to run under Windows restricted token sandbox.
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}
