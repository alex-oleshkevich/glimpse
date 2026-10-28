mod app;
mod applet;
mod applets;
mod attached;
mod cli;
mod clipboard_provider;
mod components;
mod config_alert;
mod selection;
mod services;
use anyhow::Result;
use clap::Parser;
use cli::Cli;
use glimpse_config::{Documents, PANEL_STYLESHEET};
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
    glimpse_config::init(cli.config.config_dir.clone());
    glimpse_config::seed_user_config();
    let (mut config, failure) = glimpse_config::load_or_default(Documents::ConfigAndPanel);
    let mut panels_only = false;
    if failure.is_some() {
        let (base, base_failure) = glimpse_config::load_or_default(Documents::Config);
        panels_only = base_failure.is_none();
        config = base;
    }
    let theme =
        glimpse_config::load_theme(&config.appearance.theme, PANEL_STYLESHEET).unwrap_or_default();
    init_translations(config.regional.language());
    if RELM_THREADS.set(4).is_err() {
        tracing::warn!("RELM_THREADS already initialized");
    }

    register_resources()?;
    let app_id = crate::applet::app_id();
    let app = RelmApp::new(app_id.as_str()).visible_on_activate(false);
    #[cfg(all(debug_assertions, feature = "gtk-mcp"))]
    gtk_mcp_agent::attach(&relm4::main_application());
    app.with_args(vec![]).run::<app::App>(app::AppInit {
        config,
        theme,
        failure: failure.map(|error| error.to_string()),
        panels_only,
    });
    Ok(())
}
