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
mod readiness;
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
use glimpse_config::{Documents, LOCK_STYLESHEET};
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
    glimpse_config::init(cli.config.config_dir.clone());
    match cli.command {
        Some(Command::Lock) => runtime()?.block_on(commands::lock()),
        Some(Command::Check) => {
            let config = glimpse_config::load(Documents::Config)?;
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

const MMAP_THRESHOLD: i32 = 128 * 1024;

fn daemon(cli: Cli) -> Result<()> {
    glimpse_config::seed_user_config();
    let (config, failure) = glimpse_config::load_or_default(Documents::Config);
    let theme =
        glimpse_config::load_theme(&config.appearance.theme, LOCK_STYLESHEET).unwrap_or_default();
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

    unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, MMAP_THRESHOLD) };

    if RELM_THREADS.set(4).is_err() {
        tracing::warn!("RELM_THREADS already initialized");
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
        theme,
        failure: failure.map(|error| error.to_string()),
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
