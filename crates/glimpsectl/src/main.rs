use std::process::ExitCode;

use anyhow::{Context, Result};
use glimpse_utils::init_app_tracing;
use glimpsectl::cli::{
    AppletsCommand, Cli, ClipboardCommand, Command, ConfigCommand, IdleCommand,
    NotificationsCommand, SessionCommand, SettingsCommand, SunsetCommand, WeatherCommand,
};
use glimpsectl::errors::Exit;
use glimpsectl::{cli, commands, errors};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = cli::parse();
    cli.color.write_global();
    init_app_tracing(&cli.log.log, cli.log.log_format);

    match run(cli).await {
        Ok(()) => Exit::Ok.into(),
        Err(error) => {
            anstream::eprintln!("glimpsectl: {}", errors::message(&error));
            errors::exit(&error).into()
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let session = match cli.command.needs_session_bus() {
        true => Some(
            zbus::Connection::session()
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
        Command::Sunset(SunsetCommand::Status) => commands::sunset_status(bus()?, cli.json).await,
        Command::Sunset(SunsetCommand::Pause) => commands::sunset_pause(bus()?).await,
        Command::Sunset(SunsetCommand::Resume) => commands::sunset_resume(bus()?).await,
        Command::Sunset(SunsetCommand::Mode { mode }) => commands::sunset_mode(bus()?, mode).await,
        Command::Sunset(SunsetCommand::Temperature { kelvin }) => {
            commands::sunset_temperature(bus()?, kelvin).await
        }
        Command::Idle(IdleCommand::Status) => commands::idle_status(bus()?, cli.json).await,
        Command::Idle(IdleCommand::Inhibit { lapse }) => {
            commands::idle_inhibit(bus()?, lapse).await
        }
        Command::Idle(IdleCommand::Release { id }) => commands::idle_release(bus()?, id).await,
        Command::Weather(WeatherCommand::Status) => {
            commands::weather_status(bus()?, cli.json).await
        }
        Command::Weather(WeatherCommand::Refresh) => commands::weather_refresh(bus()?).await,
        Command::Settings(SettingsCommand::Status) => {
            commands::settings_status(bus()?, cli.json).await
        }
        Command::Settings(SettingsCommand::Mode { mode }) => {
            commands::settings_mode(bus()?, mode).await
        }
        Command::Settings(SettingsCommand::Resume) => commands::settings_resume(bus()?).await,
        Command::Notifications(NotificationsCommand::List) => {
            commands::notifications_list(bus()?, cli.json).await
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
            commands::clipboard_get(bus()?, entry.id).await
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
        Command::Clipboard(ClipboardCommand::Forget(entry)) => {
            commands::clipboard_forget(bus()?, entry.id).await
        }
        Command::Clipboard(ClipboardCommand::Clear) => commands::clipboard_clear(bus()?).await,
        Command::Config(ConfigCommand::Show) => commands::config_show(cli.config.config, cli.json),
        Command::Config(ConfigCommand::Validate { path }) => {
            commands::config_validate(path.or(cli.config.config))
        }
        Command::Config(ConfigCommand::Path) => commands::config_path(cli.config.config, cli.json),
        Command::Applets(AppletsCommand::List) => {
            commands::applets_list(cli.config.config, cli.json).await
        }
        Command::Applets(AppletsCommand::New(args)) => commands::applets_new(args),
        Command::Applets(AppletsCommand::Inspect { id }) => {
            commands::applets_inspect(cli.config.config, id, cli.json).await
        }
        Command::Applets(AppletsCommand::Restart { id }) => commands::applets_restart(id).await,
        Command::Applets(AppletsCommand::Run(args)) => commands::applets_run(args).await,
        Command::Status => commands::status(bus()?, cli.json).await,
        Command::Panel(args) => commands::panel(bus()?, &args.app_id, args.command, cli.json).await,
        Command::Doctor => commands::doctor(cli.config.config, cli.json).await,
        Command::Session(SessionCommand::Declare) => commands::session_declare(bus()?).await,
    }
}
