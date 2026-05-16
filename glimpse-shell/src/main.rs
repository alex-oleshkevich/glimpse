mod agents;
mod app;
mod applet_scaffold;
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
            let rest = argv[1..].to_vec();
            if rest.iter().any(|a| a == "--help" || a == "-h") {
                print_watch_help();
                return Ok(());
            }
            let json = rest.iter().any(|a| a == "--json");
            let patterns: Vec<String> = rest.into_iter().filter(|a| a != "--json").collect();
            let patterns = if patterns.is_empty() { vec!["*".to_owned()] } else { patterns };
            return run_async(ipc::cli::watch(ipc::cli::WatchArgs { patterns, json }));
        }
        Some("dispatch") => {
            let rest_raw = &argv[1..];
            if rest_raw.iter().any(|a| a == "--help" || a == "-h") {
                print_dispatch_help();
                return Ok(());
            }
            let json = rest_raw.iter().any(|a| a == "--json");
            let rest: Vec<String> = rest_raw.iter().filter(|a| a.as_str() != "--json").cloned().collect();
            let command = rest
                .first()
                .ok_or_else(|| anyhow::anyhow!("dispatch: command name required"))?
                .clone();
            if let Some((cmd, _)) = command.split_once('=') {
                eprintln!("glimpse-shell: dispatch command must not contain '='");
                eprintln!("  got:  dispatch {command}");
                eprintln!("  try:  dispatch {cmd} <key>=<value>");
                eprintln!("Run 'glimpse-shell dispatch --help' for available commands.");
                std::process::exit(1);
            }
            let fields = rest[1..].to_vec();
            return run_async(ipc::cli::dispatch(ipc::cli::DispatchArgs { command, fields, json }));
        }
        Some("applets") => {
            match argv.get(1).map(String::as_str) {
                Some("ls") => {
                    let json = argv[2..].iter().any(|a| a == "--json");
                    return list_applets(json);
                }
                Some("new") => {
                    return applet_scaffold::run(&argv[2..]);
                }
                Some("--help") | Some("-h") | None => {
                    print_applets_help();
                    return Ok(());
                }
                Some(other) => {
                    eprintln!("glimpse-shell: unknown applets subcommand '{other}'");
                    eprintln!("Try 'glimpse-shell applets --help'.");
                    std::process::exit(1);
                }
            }
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

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// `applets ls` — a pure filesystem scan (no daemon needed) of the system and
/// user applet dirs, listing every discovered package with its provenance.
fn list_applets(json: bool) -> Result<()> {
    let applets = glimpse_core::AppletDirectoryScanner::from_process().scan_sources();

    if json {
        let body: Vec<String> = applets
            .iter()
            .map(|a| {
                format!(
                    r#"{{"id":"{}","type":"{}","source":"{}"}}"#,
                    json_escape(&a.id),
                    json_escape(&a.kind),
                    a.source
                )
            })
            .collect();
        println!("[{}]", body.join(","));
        return Ok(());
    }

    if applets.is_empty() {
        println!("no applets found");
        return Ok(());
    }

    let id_w = applets.iter().map(|a| a.id.len()).max().unwrap_or(2).max(2);
    let ty_w = applets.iter().map(|a| a.kind.len()).max().unwrap_or(4).max(4);
    println!("{:<id_w$}  {:<ty_w$}  SOURCE", "ID", "TYPE");
    for a in &applets {
        println!("{:<id_w$}  {:<ty_w$}  {}", a.id, a.kind, a.source);
    }
    Ok(())
}

fn print_help() {
    println!("glimpse-shell {}", env!("CARGO_PKG_VERSION"));
    println!("Glimpse Wayland status bar");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("    watch      Subscribe to shell events from the running daemon");
    println!("    dispatch   Send a command to the running daemon");
    println!("    applets    Inspect discovered applet packages");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help      Print help");
    println!("    -V, --version   Print version");
    println!();
    println!("Without a command, glimpse-shell starts the Wayland panel.");
    println!("Run 'glimpse-shell <COMMAND> --help' for subcommand help.");
}

fn print_watch_help() {
    println!("glimpse-shell-watch");
    println!("Subscribe to shell events from the running daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell watch [OPTIONS] [<pattern>...]");
    println!();
    println!("ARGS:");
    println!("    <pattern>...   Event patterns to subscribe to (default: *)");
    println!("                   Forms: '*', 'audio.*', 'panel.applet_added'");
    println!();
    println!("OPTIONS:");
    println!("    --json      Print each event as a JSON object");
    println!("    -h, --help  Print help");
    println!();
    println!("EVENTS:");
    println!("    audio.*          volume/mute/device/stream changes");
    println!("    network.*        connectivity/wifi/vpn/adapter changes");
    println!("    bluetooth.*      power/scan/device/health changes");
    println!("    battery.*        level/charge/peripheral changes");
    println!("    brightness.*     source add/remove and percent changes");
    println!("    power.*          profile and performance changes");
    println!("    notification.*   received/closed/dnd/health changes");
    println!("    mpris.*          player/playback/track/capability changes");
    println!("    clipboard.*      clipboard content and history changes");
    println!("    theme.*          effective theme mode changes");
    println!("    input.*          keyboard layout/availability changes");
    println!("    storage.*        device mount/eject/busy/error changes");
    println!("    webcam.* mic.*   capture device in-use changes");
    println!("    compositor.* window.* monitor.* screencast.*");
    println!("    panel.* applet.* panel/applet lifecycle + discovery");
    println!("    location.* solar.* idle.* tray.* session.* calendar.*");
}

fn print_dispatch_help() {
    println!("glimpse-shell-dispatch");
    println!("Send a command to the running daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell dispatch [OPTIONS] <COMMAND> [key=value...]");
    println!();
    println!("OPTIONS:");
    println!("    --json      Print the ack as a JSON object");
    println!("    -h, --help  Print help");
    println!();
    println!("COMMANDS:");
    println!("    status                                      Show current state snapshot");
    println!("    set_volume level=<0-100>                    Set output volume");
    println!("    toggle_mute                                 Toggle output mute");
    println!("    set_input_volume level=<0-100>              Set input volume");
    println!("    toggle_input_mute                           Toggle input mute");
    println!("    set_brightness percent=<0-100> [id=<src>]   Set brightness");
    println!("    adjust_brightness delta=<i32> [id=<src>]    Adjust brightness by delta");
    println!("    set_power_profile profile=<name>            Set power profile");
    println!("    set_dnd enabled=<bool>                      Toggle do-not-disturb");
    println!("    dismiss_notification id=<u32>               Dismiss a notification");
    println!("    dismiss_all_notifications                   Dismiss all notifications");
    println!("    media_play_pause [player=<id>]              Play/pause current media");
    println!("    media_next [player=<id>]                    Skip to next track");
    println!("    media_previous [player=<id>]                Skip to previous track");
    println!("    set_theme mode=<light|dark|auto>            Set theme mode");
    println!("    next_keyboard_layout                        Cycle to next layout");
    println!("    prev_keyboard_layout                        Cycle to previous layout");
    println!("    set_keyboard_layout index=<n>               Set layout by index");
    println!("    set_wifi enabled=<bool>                     Enable/disable Wi-Fi");
    println!("    wifi_scan                                   Trigger a Wi-Fi scan");
    println!("    connect_wifi ssid=<s> path=<p>              Connect to a Wi-Fi network");
    println!("    set_bluetooth enabled=<bool>                Enable/disable Bluetooth");
    println!("    bluetooth_scan action=<start|stop>          Start/stop discovery");
    println!("    connect_bluetooth address=<a>               Connect a Bluetooth device");
    println!("    disconnect_bluetooth address=<a>            Disconnect a Bluetooth device");
    println!("    refresh service=<battery|brightness|power|storage>  Re-poll a service");
    println!("    forget_wifi uuid=<u> confirm=true           Forget a network (destructive)");
    println!("    forget_bluetooth address=<a> confirm=true   Unpair a device (destructive)");
    println!("    eject id=<id> confirm=true                  Eject media (destructive)");
    println!("    poweroff_drive id=<id> confirm=true         Power off a drive (destructive)");
    println!("    clear_clipboard confirm=true                Clear clipboard (destructive)");
    println!("    clear_clipboard_history confirm=true        Clear history (destructive)");
}

fn print_applets_help() {
    println!("glimpse-shell-applets");
    println!("Inspect and scaffold applet packages");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell applets <SUBCOMMAND>");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help  Print help");
    println!();
    println!("SUBCOMMANDS:");
    println!("    ls [--json]                List discovered packages with a");
    println!("                               system|user|dev qualifier");
    println!("    new <name> [OPTIONS]       Scaffold a new applet project");
    println!("                               (--lang, --type, --dir, --force)");
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
