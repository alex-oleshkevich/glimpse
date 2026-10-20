mod cli;
mod errors;
mod idle;
mod inhibitors;
mod services;
mod wayland_notify;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use futures_util::StreamExt;
use glimpse_utils::{init_app_tracing, init_locale};
use services::IdleServices;
use tokio::signal::unix::{SignalKind, signal};

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

    let (mut config, failure) = glimpse_config::load_or_default(cli.config.as_deref());
    let failure = failure.map(|error| error.to_string());
    if failure.is_some() {
        config.idle.enabled = false;
    }
    tracing::info!(
        config_path = ?cli.config.config,
        enabled = config.idle.enabled,
        "glimpse-idle starting"
    );

    let services = IdleServices::start(&config, failure.clone().map_or(Ok(()), Err)).await?;
    let mut configs = Box::pin(glimpse_config::watch_config_reporting(
        cli.config.config.clone(),
        config,
        failure,
    ));
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut watching = true;

    loop {
        tokio::select! {
            _ = terminate.recv() => {
                tracing::info!(signal = "SIGTERM", "shutdown requested");
                break;
            }
            _ = interrupt.recv() => {
                tracing::info!(signal = "SIGINT", "shutdown requested");
                break;
            }
            reloaded = configs.next(), if watching => match reloaded {
                Some(Ok(Some(reloaded))) => {
                    services.reconfigure(&reloaded).await;
                    services.report(Ok(())).await;
                }
                Some(Ok(None)) => services.report(Ok(())).await,
                Some(Err(reason)) => services.report(Err(reason)).await,
                None => {
                    tracing::error!("the configuration is no longer being watched");
                    watching = false;
                }
            },
        }
    }

    services.shutdown().await;
    tracing::info!("glimpse-idle stopped");
    Ok(())
}
