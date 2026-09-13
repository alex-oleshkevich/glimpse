mod broker;
mod cli;
mod daemon;
mod errors;
mod handler;
mod legacy_services;
mod reload;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use glimpse_utils::{init_app_tracing, init_locale};

use crate::daemon::{Daemon, DaemonError, Filter};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();

    match run(cli).await {
        Ok(()) => errors::Exit::Ok.into(),
        Err(error) => {
            tracing::error!("{error:#}");
            errors::exit(&error).into()
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);
    init_locale();

    let config = glimpse_config::load(cli.config.as_deref())
        .map_err(|e| DaemonError::Config(e.to_string()))?;
    let socket = glimpse_ipc::socket_path(cli.socket.as_deref())
        .map_err(|e| DaemonError::Socket(e.to_string()))?;
    tracing::info!(path = ?socket, "using socket");

    let filter = Filter {
        only: cli.only,
        without: cli.without,
    };

    Daemon::new(filter)
        .run(&socket, config, cli.config.config)
        .await?;

    Ok(())
}
