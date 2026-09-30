mod app;
mod applet;
mod applets;
mod cli;
mod clipboard_provider;
mod components;
mod config_alert;
mod selection;
mod services;
use anyhow::Result;
use clap::Parser;
use cli::Cli;
use glimpse_utils::{init_app_tracing, init_translations};
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
    glimpse_config::seed_user_config(cli.config.as_deref());
    let (config, failure) = glimpse_config::load_or_default(cli.config.as_deref());
    init_translations(config.regional.language());
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
    let app_id = crate::applet::app_id();
    let app = RelmApp::new(app_id.as_str()).visible_on_activate(false);
    app.with_args(vec![]).run::<app::App>(app::AppInit {
        config,
        config_path: cli.config.config.clone(),
        failure: failure.map(|error| error.to_string()),
    });
    Ok(())
}
