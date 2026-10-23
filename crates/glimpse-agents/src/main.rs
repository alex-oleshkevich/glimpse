mod cli;
mod commands;
mod errors;

use std::collections::HashSet;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use clap::Parser;
use cli::{Cli, Command};
use glimpse_agents::{app, dialog};
use glimpse_config::{Documents, PANEL_STYLESHEET};
use glimpse_utils::{init_app_tracing, init_translations};
use gtk4::prelude::ApplicationExt;
use relm4::{RELM_THREADS, RelmApp};
use tokio::signal::unix::{SignalKind, signal};

const NAME: &str = "me.aresa.Glimpse.Agents";

fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();
    match run(cli) {
        Ok(()) => errors::Exit::Ok.into(),
        Err(error) => {
            tracing::error!("{error:#}");
            errors::exit(&error).into()
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);
    glimpse_config::init(cli.config.config_dir.clone());
    match cli.command {
        Some(Command::Check) => tokio::runtime::Runtime::new()?.block_on(commands::check()),
        None => daemon(),
    }
}

fn daemon() -> Result<()> {
    let (config, failure) = glimpse_config::load_or_default(Documents::Config);
    if let Some(error) = failure {
        tracing::warn!("{error}");
    }
    init_translations(config.regional.language());

    let runtime = tokio::runtime::Runtime::new()?;
    let (input, inbox) = relm4::channel();
    let (session, system) = runtime.block_on(connect(input.clone()))?;
    let cancelled = Arc::new(Mutex::new(HashSet::new()));
    runtime.spawn(glimpse_agents::agents::spawn_agents(
        system.clone(),
        input,
        Arc::clone(&cancelled),
    ));

    let theme =
        glimpse_config::load_theme(&config.appearance.theme, PANEL_STYLESHEET).unwrap_or_default();
    gtk4::init().context("cannot initialize GTK")?;
    glimpse_widgets::register_resources()?;
    dialog::register_resources()?;
    if RELM_THREADS.set(4).is_err() {
        tracing::warn!("RELM_THREADS already initialized");
    }
    runtime.block_on(watch_shutdown())?;

    RelmApp::new("me.aresa.GlimpseAgents")
        .visible_on_activate(false)
        .with_args(Vec::new())
        .run::<app::App>(app::Init {
            config,
            theme,
            session,
            system,
            runtime: runtime.handle().clone(),
            inbox,
            cancelled,
        });
    Ok(())
}

async fn watch_shutdown() -> Result<()> {
    let mut terminate = signal(SignalKind::terminate()).context("watch SIGTERM")?;
    let mut interrupt = signal(SignalKind::interrupt()).context("watch SIGINT")?;
    let _watcher = tokio::spawn(async move {
        tokio::select! {
            _ = terminate.recv() => {
                tracing::info!(signal = "SIGTERM", "shutdown requested");
            }
            _ = interrupt.recv() => {
                tracing::info!(signal = "SIGINT", "shutdown requested");
            }
        }
        glib::idle_add_once(|| {
            relm4::main_application().quit();
        });
    });
    Ok(())
}

async fn connect(
    input: relm4::Sender<glimpse_agents::ask::Input>,
) -> Result<(zbus::Connection, zbus::Connection)> {
    let session = zbus::Connection::session()
        .await
        .context("connect to the session bus")?;
    session
        .object_server()
        .at(
            glimpse_dbus::session::SESSION_PATH,
            glimpse_agents::session::Server::new(input),
        )
        .await
        .context("export the session interface")?;
    glimpse_dbus::own_name(&session, NAME)
        .await
        .with_context(|| format!("cannot own {NAME}; another glimpse-agents is running"))?;
    let system = zbus::Connection::system()
        .await
        .context("connect to the system bus")?;
    Ok((session, system))
}
