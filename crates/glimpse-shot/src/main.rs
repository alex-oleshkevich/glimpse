mod adapters;
mod cli;
mod errors;
mod finish;
mod gui;
mod headless;
mod hold;
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
    let command = match cli.command {
        Some(command) if !opens_app(Some(&command)) => command,
        _ => return open_editor(&cli.gui, cli.config.as_deref(), terminal),
    };
    match command {
        Command::Capture(args) => capture(&args, cli.config.as_deref(), terminal),
        Command::Prime => cli::prime::print(&mut std::io::stdout().lock()),
        Command::List { json } => {
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
        Command::Hold => hold::run(cli.config.as_deref()),
        Command::Portal => portal::run(cli.config.as_deref()),
    }
}

fn opens_app(command: Option<&Command>) -> bool {
    match command {
        None => true,
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
    let snapshot = match compositor_snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) if matches!(target, Target::Screen(_)) => {
            tracing::warn!("continuing without the compositor: {error:#}");
            None
        }
        Err(error) => return Err(error),
    };
    let screens = WaylandScreens {
        compositor: snapshot.is_some(),
        focused: snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_output.clone()),
        counted: RefCell::default(),
    };
    let clipboard = HoldClipboard {
        config: config_path.map(Path::to_path_buf),
    };
    let windows = snapshot.clone().map(|snapshot| CompositorWindows {
        compositor: detect_compositor(),
        snapshot,
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
mod tests {
    use clap::Parser as _;

    use super::*;

    fn editor_exit(args: &[&str]) -> Exit {
        let cli = Cli::try_parse_from(std::iter::once("glimpse-shot").chain(args.iter().copied()))
            .expect("parses");
        let terminal = Terminal {
            stdin: false,
            stdout: false,
        };
        errors::exit(
            &open_editor(&cli.gui, None, terminal).expect_err("refused before any capture"),
        )
    }

    #[test]
    fn only_the_default_command_constructs_the_app() {
        let command = |args: &[&str]| {
            Cli::try_parse_from(std::iter::once("glimpse-shot").chain(args.iter().copied()))
                .expect("parses")
                .command
        };
        assert!(opens_app(command(&[]).as_ref()));
        assert!(opens_app(command(&["--screen", "--json"]).as_ref()));
        assert!(opens_app(command(&["--gapplication-service"]).as_ref()));
        for args in [
            &["capture"][..],
            &["list"][..],
            &["prime"][..],
            &["portal"][..],
            &["hold"][..],
        ] {
            assert!(!opens_app(command(args).as_ref()), "{args:?}");
        }
    }

    #[test]
    fn default_command_usage_rows_exit_two_and_the_rest_exit_one() {
        assert_eq!(editor_exit(&["--json"]), Exit::Usage);
        assert_eq!(editor_exit(&["--prompt", "where?"]), Exit::Usage);
        assert_eq!(editor_exit(&["--timeout", "30"]), Exit::Usage);
        assert_eq!(editor_exit(&["--annotate", "marks.json"]), Exit::Usage);
        assert_eq!(
            editor_exit(&["--window", "--annotate", "marks.json"]),
            Exit::Usage
        );
    }

    #[test]
    fn a_usage_error_exits_two_before_the_configuration_is_read() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let config = dir.path().join("config.toml");
        std::fs::write(&config, "[screenshot\n").expect("writes");
        let terminal = Terminal {
            stdin: false,
            stdout: false,
        };
        let exit_of = |args: &[&str]| {
            let cli =
                Cli::try_parse_from(std::iter::once("glimpse-shot").chain(args.iter().copied()))
                    .expect("parses");
            let Some(Command::Capture(args)) = cli.command else {
                panic!("expected capture");
            };
            errors::exit(&capture(&args, Some(&config), terminal).expect_err("refused"))
        };

        assert_eq!(
            exit_of(&["capture", "--region", "garbage", "--json"]),
            Exit::Usage
        );
        assert_eq!(exit_of(&["capture", "--window", "firefox"]), Exit::Usage);
        assert_eq!(exit_of(&["capture", "-o", "-", "--json"]), Exit::Usage);
        assert_eq!(exit_of(&["capture"]), Exit::Config);
    }
}
