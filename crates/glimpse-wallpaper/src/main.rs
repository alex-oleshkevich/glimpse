mod app;
mod cli;
mod portal;
mod resolve;
mod surface;
use anyhow::Result;
use clap::Parser;
use cli::Cli;
use glimpse_utils::{init_app_tracing, init_translations};
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
    let (config, _) = glimpse_config::load_or_default(cli.config.as_deref());
    init_translations(config.regional.language());

    if RELM_THREADS.set(4).is_err() {
        tracing::warn!("RELM_THREADS already initialized");
    }

    let portal_config = cli.config.config.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("glimpse-wallpaper-portal".into())
        .spawn(move || portal::run(portal_config))
    {
        tracing::warn!(%error, "cannot start the wallpaper portal");
    }

    let app_id =
        std::env::var("GLIMPSE_WALLPAPER_APP_ID").unwrap_or("me.aresa.GlimpseWallpaper".into());
    let app = RelmApp::new(app_id.as_str()).visible_on_activate(false);
    app.with_args(vec![]).run::<app::App>(app::AppInit {
        config,
        config_path: cli.config.config.clone(),
    });
    Ok(())
}
