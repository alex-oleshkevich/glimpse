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
use glimpse_config::{Documents, Reload};
use glimpse_utils::{init_app_tracing, init_locale};
use services::SunsetServices;
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
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

    glimpse_config::init(cli.config.config_dir.clone());
    let (mut config, failure) = glimpse_config::load_or_default(Documents::Config);
    let failure = failure.map(|error| error.to_string());
    if failure.is_some() {
        config.night_light.schedule = glimpse_config::Schedule::Off;
    }
    tracing::info!(
        config_dir = ?cli.config.config_dir,
        schedule = ?config.night_light.schedule,
        temperature = config.night_light.temperature,
        "glimpse-sunset starting"
    );

    let services = SunsetServices::start(&config, failure.clone().map_or(Ok(()), Err)).await?;
    let mut configs = Box::pin(glimpse_config::watch_config(
        Documents::Config,
        config,
        failure,
    ));
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut watching = true;
    let mut bus_closed = Box::pin(services.bus_closed());

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
            () = &mut bus_closed => {
                tracing::info!("the session bus is gone; shutting down");
                break;
            }
            reloaded = configs.next(), if watching => match reloaded {
                Some(Reload::Changed(reloaded)) => {
                    services.reconfigure(&reloaded);
                    services.report(Ok(())).await;
                }
                Some(Reload::Recovered) => services.report(Ok(())).await,
                Some(Reload::Rejected(reason)) => services.report(Err(reason)).await,
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
