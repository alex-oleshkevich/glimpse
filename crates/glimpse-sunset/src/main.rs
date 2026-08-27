mod cli;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use futures_util::StreamExt;
use glimpse_config::watch_config;
use glimpse_ipc::Client;
use glimpse_utils::init_app_tracing;
use tokio::signal::unix::{SignalKind, signal};

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

    let mut config = glimpse_config::load(cli.config.as_deref())?;
    let socket = glimpse_ipc::socket_path(cli.socket.as_deref())?;
    let _client = Client::open(&socket);

    let mut configs = Box::pin(watch_config(cli.config.config.clone(), config.clone()));
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut reloading = true;

    loop {
        tokio::select! {
            _ = terminate.recv() => break,
            _ = interrupt.recv() => break,
            reloaded = configs.next(), if reloading => match reloaded {
                Some(reloaded) => {
                    config = reloaded;
                    tracing::info!(night_light = ?config.night_light, "configuration reloaded");
                }
                None => {
                    tracing::error!("the configuration is no longer being watched");
                    reloading = false;
                }
            },
        }
    }

    Ok(())
}
