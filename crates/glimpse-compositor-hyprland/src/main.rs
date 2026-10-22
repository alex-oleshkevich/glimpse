mod cli;
mod hyprland;

use std::process::ExitCode;

use clap::Parser;
use cli::Cli;
use glimpse_compositors::provider;
use glimpse_utils::init_app_tracing;
use hyprland::Hyprland;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    init_app_tracing(&cli.log.log, cli.log.log_format);
    provider::run(
        cli.probe,
        Hyprland::from_env(),
        "no Hyprland instance socket directory",
    )
    .await
}
