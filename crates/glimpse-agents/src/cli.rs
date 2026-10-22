use clap::{Parser, Subcommand};
use glimpse_utils::{ConfigArg, LogArgs};
use shadow_rs::shadow;

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-agents",
    about = "Polkit, NetworkManager and BlueZ agents for the glimpse session.",
    version = build::VERSION
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Check that every agent can reach what it needs")]
    Check,
}

#[cfg(test)]
mod tests;
