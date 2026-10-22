mod app;
mod cli;
mod errors;
mod provider;
mod services;
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
    glimpse_config::init(cli.config.config_dir.clone());
    glimpse_config::seed_user_config();
    let (config, failure) = glimpse_config::load_or_default(glimpse_config::Documents::Config);
    let theme =
        glimpse_config::load_theme(&config.appearance.theme, glimpse_config::PANEL_STYLESHEET)
            .unwrap_or_default();
    init_translations(config.regional.language());
    gtk4::init().context("cannot initialize GTK")?;
    glimpse_widgets::register_resources()?;
    if RELM_THREADS.set(4).is_err() {
        tracing::warn!("RELM_THREADS already initialized");
    }

    let app_id = std::env::var("GLIMPSE_NOTIFICATIONS_APP_ID")
        .unwrap_or_else(|_| "me.aresa.GlimpseNotifications".to_owned());
    RelmApp::new(&app_id)
        .with_args(Vec::new())
        .run::<app::App>(app::Init {
            config,
            theme,
            failure: failure.map(|error| error.to_string()),
        });
    Ok(())
}
