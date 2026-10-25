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
use std::process::ExitCode;

use anyhow::{Result, anyhow};
use cli::{CaptureArgs, Cli, Command, GuiArgs, Terminal};
use errors::{Exit, Usage};
use glimpse_config::Documents;
use glimpse_shot::ports::Windows;
use glimpse_shot::session::Target;
use glimpse_utils::init_app_tracing;
use gtk4::glib;

use crate::adapters::clipboard::HoldClipboard;
use crate::adapters::clock::SystemClock;
use crate::adapters::files::Disk;
use crate::adapters::screens::{self, WaylandScreens};
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

fn run(
    Cli {
        config,
        log,
        gui,
        command,
        ..
    }: Cli,
) -> Result<()> {
    glimpse_config::init(config.config_dir);
    init_app_tracing(&log.log, log.log_format);
    let terminal = Terminal {
        stdin: std::io::stdin().is_terminal(),
        stdout: std::io::stdout().is_terminal(),
    };
    match command {
        None => open_editor(&gui, terminal),
        Some(Command::Capture(args)) => capture(&args, terminal),
        Some(Command::Annotate(args)) => {
            cli::validate_annotate(&args, terminal).map_err(Usage)?;
            if !args.headless {
                return gui::app::run(&args.into_gui());
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
                terminal,
            )
        }
        Some(Command::Prime) => cli::prime::print(&mut std::io::stdout().lock()),
        Some(Command::List { json }) => {
            let layout = screens::layout()?.ok_or_else(|| anyhow!(headless::NO_WINDOWS))?;
            headless::list(
                &layout,
                json,
                terminal.stdout,
                &mut std::io::stdout().lock(),
            )
        }
        Some(Command::Hold) => hold::run(),
        Some(Command::Portal) => portal::run(),
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

fn capture(args: &CaptureArgs, terminal: Terminal) -> Result<()> {
    cli::validate_capture(args, terminal).map_err(Usage)?;
    let target = args.target.resolve(true).map_err(Usage)?;
    let config = glimpse_config::load(Documents::Config)?;
    let layout = match args.image.as_ref() {
        Some(_) => None,
        None => match screens::layout() {
            Ok(layout) => layout,
            Err(error) if matches!(target, Target::Screen(_)) => {
                tracing::warn!("continuing without the compositor: {error:#}");
                None
            }
            Err(error) => return Err(error),
        },
    };
    let screens = WaylandScreens {
        compositor: if layout.is_some() {
            screens::session()?
        } else {
            None
        },
        counted: RefCell::default(),
    };
    let clipboard = HoldClipboard;
    let windows = layout.clone().map(|layout| CompositorWindows {
        layout: RefCell::new(layout),
        clipboard: &clipboard,
        restore: !args.copy,
    });
    let deps = headless::Deps {
        screens: &screens,
        windows: windows.as_ref().map(|windows| windows as &dyn Windows),
        clipboard: &clipboard,
        files: &Disk,
        clock: &SystemClock,
        outputs: layout.as_ref().map(headless::placed).unwrap_or_default(),
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

fn open_editor(gui: &GuiArgs, terminal: Terminal) -> Result<()> {
    cli::validate_gui(gui, terminal).map_err(Usage)?;
    gui::app::run(gui)
}

#[cfg(test)]
mod tests;
