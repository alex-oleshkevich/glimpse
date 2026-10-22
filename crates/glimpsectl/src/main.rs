use std::process::ExitCode;

use anyhow::{Context, Result};
use glimpse_utils::init_app_tracing;
use glimpsectl::cli::*;
use glimpsectl::errors::Exit;
use glimpsectl::{cli, commands, errors};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = cli::parse();
    glimpse_config::init(cli.config.config_dir.clone());
    cli.color.write_global();
    init_app_tracing(&cli.log.log, cli.log.log_format);
    let json = cli.json;

    match run(cli).await {
        Ok(()) => Exit::Ok.into(),
        Err(error) => {
            let code = error
                .downcast_ref::<errors::ChildExit>()
                .map_or_else(|| errors::exit(&error) as u8, |exit| exit.0);
            let message = errors::message(&error);
            anstream::eprintln!("glimpsectl: {message}");
            if json {
                anstream::eprintln!("{}", serde_json::json!({"error": message, "exit": code}));
            }
            ExitCode::from(code)
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let session = match cli.command.needs_session_bus() {
        true => Some(
            commands::within(zbus::Connection::session())
                .await
                .context("cannot reach the session bus")?,
        ),
        false => None,
    };
    let bus = || {
        session
            .as_ref()
            .context("this command needs the session bus")
    };

    match cli.command {
        Command::NightLight(command) => match command {
            NightLightCommand::Status { watch: false } => {
                commands::sunset_status(bus()?, cli.json).await
            }
            NightLightCommand::Status { watch: true } => {
                commands::sunset::sunset_watch(bus()?, cli.json).await
            }
            NightLightCommand::Pause => commands::sunset_pause(bus()?).await,
            NightLightCommand::Resume => commands::sunset_resume(bus()?).await,
            NightLightCommand::Schedule { mode } => commands::sunset_mode(bus()?, mode).await,
            NightLightCommand::Temperature { kelvin, .. } => {
                commands::sunset_temperature(bus()?, kelvin.unwrap_or(0)).await
            }
        },
        Command::Idle(IdleCommand::Status { watch: false }) => {
            commands::idle_status(bus()?, cli.json).await
        }
        Command::Idle(IdleCommand::Status { watch: true }) => {
            commands::idle::idle_watch(bus()?, cli.json).await
        }
        Command::Idle(IdleCommand::Inhibit { lapse, command }) => {
            commands::idle::idle_inhibit_child(bus()?, lapse, &command, cli.json).await
        }
        Command::Idle(IdleCommand::Release { id }) => commands::idle_release(bus()?, id).await,
        Command::Weather(WeatherCommand::Status) => {
            commands::weather_status(bus()?, cli.json).await
        }
        Command::Weather(WeatherCommand::Refresh) => commands::weather_refresh(bus()?).await,
        Command::Appearance(command) => match command {
            AppearanceCommand::Status { watch: false } => {
                commands::settings::appearance_status(bus()?, cli.json).await
            }
            AppearanceCommand::Status { watch: true } => {
                commands::settings::appearance_watch(bus()?, cli.json).await
            }
            AppearanceCommand::Set { mode } => commands::settings_mode(bus()?, mode).await,
            AppearanceCommand::Resume => commands::settings_resume(bus()?).await,
        },
        Command::Geolocation(GeolocationCommand::Status) => {
            commands::settings::geolocation_status(bus()?, cli.json).await
        }
        Command::Notifications(NotificationsCommand::List { watch: false }) => {
            commands::notifications_list(bus()?, cli.json).await
        }
        Command::Notifications(NotificationsCommand::List { watch: true }) => {
            commands::notifications::notifications_list_watch(bus()?, cli.json).await
        }
        Command::Notifications(NotificationsCommand::Send(args)) => {
            commands::notifications::notifications_send(bus()?, &args, cli.json).await
        }
        Command::Notifications(NotificationsCommand::Remove { id }) => {
            commands::notifications::notifications_remove(bus()?, id).await
        }
        Command::Notifications(NotificationsCommand::Activate { id }) => {
            commands::notifications::notifications_activate(bus()?, id).await
        }
        Command::Notifications(NotificationsCommand::Action { id, key }) => {
            commands::notifications::notifications_action(bus()?, id, &key).await
        }
        Command::Notifications(NotificationsCommand::Dismiss { id }) => {
            commands::notifications_dismiss(bus()?, id).await
        }
        Command::Notifications(NotificationsCommand::Clear { app }) => {
            commands::notifications_clear(bus()?, app).await
        }
        Command::Notifications(NotificationsCommand::Dnd { state, until }) => {
            commands::notifications_dnd(bus()?, state, until).await
        }
        Command::Clipboard(ClipboardCommand::List) => {
            commands::clipboard_list(bus()?, cli.json).await
        }
        Command::Clipboard(ClipboardCommand::Get(entry)) => {
            commands::clipboard::clipboard_get_output(bus()?, entry.id, cli.json).await
        }
        Command::Clipboard(ClipboardCommand::Copy(entry)) => {
            commands::clipboard_copy(bus()?, entry.id).await
        }
        Command::Clipboard(ClipboardCommand::Pin(entry)) => {
            commands::clipboard_pin(bus()?, entry.id, true).await
        }
        Command::Clipboard(ClipboardCommand::Unpin(entry)) => {
            commands::clipboard_pin(bus()?, entry.id, false).await
        }
        Command::Clipboard(ClipboardCommand::Remove(entry)) => {
            commands::clipboard_forget(bus()?, entry.id).await
        }
        Command::Clipboard(ClipboardCommand::Clear) => commands::clipboard_clear(bus()?).await,
        Command::Config(ConfigCommand::Show) => commands::config_show(cli.json),
        Command::Config(ConfigCommand::Validate { path }) => {
            commands::config::config_validate_output(path, cli.json)
        }
        Command::Config(ConfigCommand::Path) => commands::config_path(cli.json),
        Command::Applet(command) => match command {
            AppletCommand::List {
                installed: true, ..
            } => commands::applets_list(cli.json).await,
            AppletCommand::Inspect {
                target,
                installed: true,
            } => commands::applets_inspect(target.name, cli.json).await,
            AppletCommand::New(args) => commands::applets_new(args, cli.json),
            AppletCommand::Run(args) => {
                commands::applets_run(args, &cli.panel_app_id, cli.json).await
            }
            AppletCommand::Restart { id } => commands::applets_restart(id).await,
            AppletCommand::Package(args) => {
                commands::package_manager::applet_package(args, cli.json)
            }
            AppletCommand::Install { target } => {
                commands::package_manager::applet_install(target, cli.json)
            }
            AppletCommand::Uninstall { id } => commands::package_manager::applet_uninstall(id),
            command => {
                commands::panel::applet_live(bus()?, &cli.panel_app_id, command, cli.json).await
            }
        },
        Command::Dev(DevCommand::Panel(args)) => {
            commands::dev::dev_panel(args, cli.config.config_dir.is_some(), cli.json).await
        }
        Command::Dev(DevCommand::ClosePopovers) => {
            commands::panel(
                bus()?,
                &cli.panel_app_id,
                PanelCommand::Popovers(PopoversCommand::Close),
                cli.json,
            )
            .await
        }
        Command::Completions { shell } => {
            let mut output = Vec::new();
            clap_complete::generate(
                shell,
                &mut cli::completion_command(),
                "glimpsectl",
                &mut output,
            );
            glimpsectl::render::print(String::from_utf8(output)?.trim_end())?;
            Ok(())
        }
        Command::Status => commands::status(bus()?, cli.json).await,
        Command::Panel(args) => {
            commands::panel(
                bus()?,
                args.app_id.as_deref().unwrap_or(&cli.panel_app_id),
                args.command,
                cli.json,
            )
            .await
        }
        Command::Doctor => commands::doctor(cli.json).await,
        Command::Session(SessionCommand::Declare) => commands::session_declare(bus()?).await,
    }
}
