mod cli;
mod errors;
mod notify;
mod provider;
mod services;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser as _;
use cli::Cli;
use futures_util::StreamExt as _;
use glimpse_config::{Documents, Reload, watch_config};
use glimpse_utils::{init_app_tracing, init_translations};
use services::CalendarServices;
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main(flavor = "current_thread")]
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
    glimpse_config::init(cli.config.config_dir.clone());
    let (config, _) = glimpse_config::load_or_default(Documents::Config);
    init_translations(config.regional.language());
    tracing::info!(
        config_dir = ?cli.config.config_dir,
        sources = config.calendar.sources.len(),
        "glimpse-calendar starting"
    );
    let services = CalendarServices::start(&config).await?;
    let mut configs = Box::pin(watch_config(Documents::Config, config, None));
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
                Some(Reload::Changed(reloaded)) => services.reconfigure(&reloaded),
                Some(Reload::Recovered | Reload::Rejected(_)) => {}
                None => {
                    tracing::error!("the configuration is no longer being watched");
                    watching = false;
                }
            },
        }
    }

    services.shutdown().await;
    tracing::info!("glimpse-calendar stopped");
    Ok(())
}
