mod cli;
mod errors;
mod mode;
mod provider;
mod services;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser as _;
use cli::Cli;
use futures_util::StreamExt as _;
use glimpse_config::{Documents, Reload};
use glimpse_utils::init_app_tracing;
use services::SettingsServices;
use tokio::signal::unix::{SignalKind, signal};

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

    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    glimpse_config::init(cli.config.config_dir.clone());
    let (config, failure) = glimpse_config::load_or_default(Documents::Config);
    let failure = failure.map(|error| error.to_string());
    let services = SettingsServices::start(&config, failure.clone().map_or(Ok(()), Err)).await?;
    let mut configs = Box::pin(glimpse_config::watch_config(
        Documents::Config,
        config,
        failure,
    ));
    let mut watching = true;

    tracing::info!(config_dir = ?cli.config.config_dir, "glimpse-settings-daemon started");

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
                Some(Reload::Changed(config)) => {
                    services.reconfigure(&config);
                    services.report(Ok(())).await;
                }
                Some(Reload::Recovered) => services.report(Ok(())).await,
                Some(Reload::Rejected(reason)) => services.report(Err(reason)).await,
                None => {
                    tracing::error!("the configuration is no longer being watched");
                    watching = false;
                }
            }
        }
    }

    services.shutdown().await;
    tracing::info!("glimpse-settings-daemon stopped");
    Ok(())
}
