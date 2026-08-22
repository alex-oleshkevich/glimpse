mod cli;
mod errors;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use glimpse_utils::init_app_tracing;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();

    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error:#}");
            errors::exit_code(&error)
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);

    let _config = glimpse_config::load(cli.config.as_deref())?;
    let _socket = glimpse_ipc::socket_path(cli.socket.as_deref())?;

    Ok(())
}
