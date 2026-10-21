mod adapters;
mod cli;
mod errors;
mod finish;
mod gui;
mod headless;
mod hold;
mod image;
mod notify;
mod portal;
mod render;

use std::cell::RefCell;
use std::io::IsTerminal as _;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Result, anyhow};
use cli::{CaptureArgs, Cli, Command, GuiArgs, Terminal};
use errors::{Exit, Usage};
use glimpse_compositors::detect_compositor;
use glimpse_shot::ports::Windows;
use glimpse_shot::session::Target;
use glimpse_utils::init_app_tracing;
use gtk4::glib;

use crate::adapters::clipboard::HoldClipboard;
use crate::adapters::clock::SystemClock;
use crate::adapters::files::Disk;
use crate::adapters::screens::{WaylandScreens, compositor_snapshot};
use crate::adapters::windows::CompositorWindows;

fn main() -> ExitCode {
    unsafe { glib::log_writer_default_set_use_stderr(true) };
    let args: Vec<_> = std::env::args_os().collect();
    let cli = match cli::parse(args.iter().cloned()) {
        Ok(cli) => cli,
        Err(error) if !error.use_stderr() => error.exit(),
        Err(error) => {
            let json = args.iter().any(|arg| arg == "--json");
            return errors::clap_failure(&error, json, &mut anstream::stderr()).into();
        }
    };
    cli.color.write_global();
    let json = cli.json();
    match run(cli) {
        Ok(()) => Exit::Ok.into(),
        Err(error) => match error.downcast_ref::<gui::app::Exited>() {
            Some(exited) => ExitCode::from(exited.0),
            None => errors::report(&error, json, &mut anstream::stderr()).into(),
        },
    }
}

fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);
    let terminal = Terminal {
        stdin: std::io::stdin().is_terminal(),
        stdout: std::io::stdout().is_terminal(),
    };
    match cli.command {
        None => open_editor(&cli.gui, cli.config.as_deref(), terminal),
        Some(Command::Capture(args)) => capture(&args, cli.config.as_deref(), terminal),
        Some(Command::Annotate(args)) => {
            cli::validate_annotate(&args, terminal).map_err(Usage)?;
            if !args.headless {
                return gui::app::run(&args.into_gui(), cli.config.as_deref());
            }
            capture(
                &CaptureArgs {
                    image: args.image,
                    output: args.output,
                    copy: args.copy,
                    no_save: args.no_save,
                    annotate: args.annotate,
                    json: args.json,
                    ..Default::default()
                },
                cli.config.as_deref(),
                terminal,
            )
        }
        Some(Command::Prime) => cli::prime::print(&mut std::io::stdout().lock()),
        Some(Command::List { json }) => {
            let snapshot = compositor_snapshot()?.ok_or_else(|| {
                anyhow!("list needs a compositor; neither NIRI_SOCKET nor HYPRLAND_INSTANCE_SIGNATURE is set")
            })?;
            headless::list(
                &snapshot,
                json,
                terminal.stdout,
                &mut std::io::stdout().lock(),
            )
        }
        Some(Command::Hold) => hold::run(cli.config.as_deref()),
        Some(Command::Portal) => portal::run(cli.config.as_deref()),
    }
}

fn opens_app(command: Option<&Command>) -> bool {
    match command {
        None => true,
        Some(Command::Annotate(args)) => !args.headless,
        Some(
            Command::Capture(_)
            | Command::Prime
            | Command::List { .. }
            | Command::Portal
            | Command::Hold,
        ) => false,
    }
}

fn capture(args: &CaptureArgs, config_path: Option<&Path>, terminal: Terminal) -> Result<()> {
    cli::validate_capture(args, terminal).map_err(Usage)?;
    let target = args.target.resolve(true).map_err(Usage)?;
    let config = glimpse_config::load(config_path)?;
    let snapshot = match args
        .image
        .as_ref()
        .map_or_else(compositor_snapshot, |_| Ok(None))
    {
        Ok(snapshot) => snapshot,
        Err(error) if matches!(target, Target::Screen(_)) => {
            tracing::warn!("continuing without the compositor: {error:#}");
            None
        }
        Err(error) => return Err(error),
    };
    let screens = WaylandScreens {
        compositor: snapshot.is_some(),
        counted: RefCell::default(),
    };
    let clipboard = HoldClipboard {
        config: config_path.map(Path::to_path_buf),
    };
    let windows = snapshot.clone().map(|snapshot| CompositorWindows {
        compositor: detect_compositor(),
        snapshot: RefCell::new(snapshot),
        clipboard: &clipboard,
        restore: !args.copy,
    });
    let deps = headless::Deps {
        screens: &screens,
        windows: windows.as_ref().map(|windows| windows as &dyn Windows),
        clipboard: &clipboard,
        files: &Disk,
        clock: &SystemClock,
        outputs: snapshot.as_ref().map(headless::placed).unwrap_or_default(),
        encode: &render::export,
        step: glimpse_shot::session::step,
    };
    headless::capture(
        args,
        &config.screenshot,
        terminal,
        &deps,
        &mut std::io::stdout().lock(),
    )
}

fn open_editor(gui: &GuiArgs, config_path: Option<&Path>, terminal: Terminal) -> Result<()> {
    cli::validate_gui(gui, terminal).map_err(Usage)?;
    gui::app::run(gui, config_path)
}

#[cfg(test)]
mod tests;
