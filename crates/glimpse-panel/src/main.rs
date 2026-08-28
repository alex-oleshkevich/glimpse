mod app;
mod cli;
mod components;
use anyhow::Result;
use clap::Parser;
use cli::Cli;
use glimpse_utils::init_app_tracing;
use glimpse_widgets::register_resources;
use relm4::{RELM_THREADS, RelmApp};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("{err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);
    let socket = glimpse_ipc::socket_path(cli.socket.as_deref())?;
    let config = glimpse_config::load(cli.config.as_deref())?;

    let threads = std::env::var("GLIMPSE_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);
    if RELM_THREADS.set(threads).is_err() {
        tracing::warn!(
            threads,
            "RELM_THREADS already initialized; GLIMPSE_THREADS ignored"
        );
    }

    register_resources()?;
    let app_id = std::env::var("GLIMPSE_PANEL_APP_ID").unwrap_or("me.aresa.GlimpsePanel".into());
    let app = RelmApp::new(app_id.as_str()).visible_on_activate(false);
    app.with_args(vec![]).run::<app::App>(app::AppInit {
        config,
        config_path: cli.config.config.clone(),
        socket,
    });
    Ok(())
}
