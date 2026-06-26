//! Command-line interface definitions.

use std::path::PathBuf;

use clap::Parser;

/// Parsed command-line configuration for the watchdog daemon.
#[derive(Parser, Debug)]
#[command(name = "wattdog")]
#[command(version)]
#[command(about = "PowerMon threshold watchdog daemon")]
pub struct Cli {
    /// TOML configuration file path.
    #[arg(long, default_value = "/etc/wattdog/config.toml")]
    pub config: PathBuf,

    /// Validate configuration and exit without starting scanner/tasks.
    #[arg(long)]
    pub check_config: bool,

    /// Parse dry-run mode for the later action phase.
    #[arg(long)]
    pub dry_run: bool,
}
