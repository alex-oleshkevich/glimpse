use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod project;

#[derive(Parser)]
#[command(
    name = "glimpse-applet",
    version,
    about = "Scaffold, run, and verify Glimpse exec applets."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new applet project.
    New(commands::new::Args),
    /// Run an applet in development mode with auto-rebuild and process restart.
    Dev(commands::dev::Args),
    /// Verify that everything you need to build applets is installed.
    Doctor(commands::doctor::Args),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::New(args) => commands::new::run(args),
        Command::Dev(args) => commands::dev::run(args).await,
        Command::Doctor(args) => commands::doctor::run(args),
    }
}
