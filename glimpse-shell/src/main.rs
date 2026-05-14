mod agents;
mod app;
mod applets;
mod components;
mod dbus;
mod ipc;
mod panels;
mod prompts;
pub mod services;
mod theme;

use anyhow::Result;
use relm4::{RELM_THREADS, RelmApp};
use tracing_subscriber::EnvFilter;

pub use glimpse_core::compositors;

use crate::{
    app::{App, AppInit},
    compositors::detect_compositor,
};
use glimpse_core::Config;
use glimpse_core::dbus::Dbus;

fn main() -> Result<()> {
    // Argv is inspected before any GTK/GLib code runs.
    let argv: Vec<String> = std::env::args().skip(1).collect();

    match argv.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            print_help();
            return Ok(());
        }
        Some("--version") | Some("-V") => {
            println!("glimpse-shell {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("watch") => {
            let patterns: Vec<String> = argv[1..].to_vec();
            let patterns = if patterns.is_empty() {
                vec!["*".to_owned()]
            } else {
                patterns
            };
            return run_async(ipc::cli::watch(ipc::cli::WatchArgs { patterns }));
        }
        Some("dispatch") => {
            let rest = &argv[1..];
            let command = rest
                .first()
                .ok_or_else(|| anyhow::anyhow!("dispatch: command name required"))?
                .clone();
            let fields = rest[1..].to_vec();
            return run_async(ipc::cli::dispatch(ipc::cli::DispatchArgs { command, fields }));
        }
        None => {}
        Some(unknown) => {
            eprintln!("glimpse-shell: unknown command '{unknown}'");
            eprintln!("Try 'glimpse-shell --help' for usage.");
            std::process::exit(1);
        }
    }

    run_shell()
}

fn run_async<F>(f: F) -> Result<()>
where
    F: std::future::Future<Output = Result<()>>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(f)
}

fn print_help() {
    println!("glimpse-shell {}", env!("CARGO_PKG_VERSION"));
    println!("Glimpse Wayland status bar");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("    watch [<pattern>...]       Subscribe to shell events (default: *)");
    println!("    dispatch <cmd> [key=val...]  Send a command to the running shell");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       Print help");
    println!("    -V, --version    Print version");
    println!();
    println!("Without a command, glimpse-shell starts the Wayland panel.");
}

fn run_shell() -> Result<()> {
    let filter = EnvFilter::try_from_env("GLIMPSE_LOG_LEVEL")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

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

    let config = Config::autodetect();
    if let Some(compositor) = detect_compositor() {
        tracing::info!(compositor = compositor.name(), "detected compositor");
    } else {
        tracing::warn!("unsupported compositor");
    }

    let dbus = Dbus::connect()?;

    let app_id = std::env::var("GLIMPSE_SHELL_APP_ID").unwrap_or("me.aresa.GlimpseShell".into());
    let app = RelmApp::new(app_id.as_str());

    register_resources();
    app.with_args(vec![]).run::<App>(AppInit { config, dbus });

    Ok(())
}

fn register_resources() {
    gio::resources_register_include!("glimpse-shell.gresource")
        .expect("failed to register embedded resources");
}
