mod app;
mod auth;
mod background;
mod chips;
mod cli;
mod commands;
mod errors;
mod lifecycle;
mod logind;
mod media;
mod notify;
mod probe;
mod session;
mod status;
mod surface;
mod user;

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};
use clap::Parser;
use cli::{Cli, Command};
use errors::Exit;
use glimpse_utils::{init_app_tracing, init_translations};
use gtk4::gio;
use gtk4::prelude::ApplicationExt;
use relm4::{RELM_THREADS, RelmApp};

fn main() -> ExitCode {
    let cli = Cli::parse().checked().unwrap_or_else(|error| error.exit());
    cli.color.write_global();

    match run(cli) {
        Ok(()) => Exit::Ok.into(),
        Err(error) => {
            tracing::error!("{error:#}");
            errors::exit(&error).into()
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);
    match cli.command {
        Some(Command::Lock) => runtime()?.block_on(commands::lock()),
        Some(Command::Check) => {
            let config = glimpse_config::load(cli.config.as_deref())?;
            let pam_service = auth::Service::new(&config.lock.pam_service);
            runtime()?.block_on(commands::check(pam_service.name()))
        }
        None => daemon(cli),
    }
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

fn daemon(cli: Cli) -> Result<()> {
    glimpse_config::seed_user_config(cli.config.as_deref());
    let config = glimpse_config::load(cli.config.as_deref())?;
    init_translations(config.regional.language());

    let pam_service = auth::Service::new(&config.lock.pam_service);
    let checks = probe::startup(pam_service.name());
    let mut cant_verify = probe::failures(&checks);
    for line in &cant_verify {
        tracing::error!(check = %line, "sandbox self-probe failed; passwords cannot be verified");
    }
    let user = user::current()
        .inspect_err(|error| {
            tracing::error!(%error, "no username to authenticate");
            cant_verify.push(error.clone());
        })
        .ok();

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

    let unlocked = Arc::new(AtomicBool::new(false));
    let app_id = std::env::var("GLIMPSE_LOCK_APP_ID").unwrap_or("me.aresa.GlimpseLock".into());
    glimpse_widgets::register_resources()?;
    let app = RelmApp::new(app_id.as_str()).visible_on_activate(false);
    relm4::main_application()
        .set_flags(relm4::main_application().flags() | gio::ApplicationFlags::NON_UNIQUE);
    app.with_args(vec![]).run::<app::App>(app::AppInit {
        config,
        pam_service,
        config_path: cli.config.config.clone(),
        standalone: cli.standalone,
        cant_verify: !cant_verify.is_empty(),
        user,
        unlocked: unlocked.clone(),
    });
    if cli.standalone && !unlocked.load(Ordering::SeqCst) {
        bail!("the standalone lock ended without an authenticated unlock");
    }
    Ok(())
}
