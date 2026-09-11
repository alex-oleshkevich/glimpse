mod app;
mod cli;
mod errors;
mod state;

use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::Parser;
use cli::Cli;
use glimpse_utils::{init_app_tracing, init_translations};
use relm4::{RELM_THREADS, RelmApp};

fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();
    match run(&cli) {
        Ok(()) => errors::Exit::Ok.into(),
        Err(error) => {
            tracing::error!("{error:#}");
            errors::exit(&error).into()
        }
    }
}

fn run(cli: &Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);
    let config = glimpse_config::load(cli.config.as_deref())?;
    init_translations(config.regional.language());
    gtk4::init().context("cannot initialize GTK")?;
    glimpse_widgets::register_resources()?;
    let socket = glimpse_ipc::socket_path(cli.socket.as_deref())?;

    let threads = std::env::var("GLIMPSE_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(4);
    if RELM_THREADS.set(threads).is_err() {
        tracing::warn!(
            threads,
            "RELM_THREADS already initialized; GLIMPSE_THREADS ignored"
        );
    }

    let app_id = std::env::var("GLIMPSE_NOTIFICATIOND_APP_ID")
        .unwrap_or_else(|_| "me.aresa.GlimpseNotificationd".to_owned());
    RelmApp::new(&app_id)
        .with_args(Vec::new())
        .run::<app::App>(app::Init {
            config,
            config_path: cli.config.config.clone(),
            socket,
        });
    Ok(())
}
