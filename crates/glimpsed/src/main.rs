mod cli;

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
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);

    if !cli.only.is_empty() {
        tracing::info!(names = ?cli.only, "running only these services");
    } else if !cli.without.is_empty() {
        tracing::info!(names = ?cli.without, "running without these services");
    }

    let _config = glimpse_config::load(cli.config.as_deref())?;
    let socket = glimpse_ipc::socket_path(cli.socket.as_deref())?;

    let _listener = glimpse_ipc::Server::listen(&socket).await?;
    tracing::info!(socket = %socket.display(), "glimpsed {}", env!("CARGO_PKG_VERSION"));
    tracing::warn!("the broker, service registry and socket server are not implemented yet");

    Ok(())
}
