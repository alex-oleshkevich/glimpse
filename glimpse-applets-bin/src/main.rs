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
    /// Symlink the current applet into the Glimpse applets directory.
    Link(commands::link::Args),
    /// Remove the symlink created by link.
    Unlink(commands::link::unlink::Args),
    /// List installed applets.
    List(commands::list::Args),
    /// Remove an installed applet.
    Rm(commands::rm::Args),
    /// Verify that everything you need to build applets is installed.
    Doctor(commands::doctor::Args),
    /// Subscribe to shell events and print them to stdout.
    Watch(commands::ipc::WatchArgs),
    /// Send a command to the shell and print the acknowledgement.
    Dispatch(commands::ipc::DispatchArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::New(args) => commands::new::run(args),
        Command::Dev(args) => commands::dev::run(args).await,
        Command::Link(args) => commands::link::run(args),
        Command::Unlink(args) => commands::link::unlink::run(args),
        Command::List(args) => commands::list::run(args),
        Command::Rm(args) => commands::rm::run(args),
        Command::Doctor(args) => commands::doctor::run(args),
        Command::Watch(args) => commands::ipc::watch(args).await,
        Command::Dispatch(args) => commands::ipc::dispatch(args).await,
    }
}
