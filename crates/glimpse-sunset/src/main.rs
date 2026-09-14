mod cli;
mod errors;
mod gamma;
mod provider;
mod services;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use futures_util::StreamExt;
use glimpse_config::watch_config;
use glimpse_utils::{init_app_tracing, init_locale};
use services::SunsetServices;
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

    let config = glimpse_config::load(cli.config.as_deref())?;
    tracing::info!(
        config_path = ?cli.config.config,
        schedule = ?config.night_light.schedule,
        temperature = config.night_light.temperature,
        "glimpse-sunset starting"
    );

    let services = SunsetServices::start(&config).await?;
    let mut configs = Box::pin(watch_config(cli.config.config.clone(), config));
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
                Some(reloaded) => services.reconfigure(&reloaded),
                None => {
                    tracing::error!("the configuration is no longer being watched");
                    watching = false;
                }
            },
        }
    }

    services.shutdown().await;
    tracing::info!("glimpse-sunset stopped");
    Ok(())
}
