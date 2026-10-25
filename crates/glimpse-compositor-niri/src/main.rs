mod cli;
mod niri;

use std::process::ExitCode;

use clap::Parser;
use cli::Cli;
use glimpse_compositors::provider;
use glimpse_utils::init_app_tracing;
use niri::Niri;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    init_app_tracing(&cli.log.log, cli.log.log_format);
    provider::run(
        cli.probe,
        Niri::from_env(),
        "NIRI_SOCKET is not set or empty",
    )
    .await
}
