mod cli;
mod commands;
mod errors;
mod render;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{
    AppletsCommand, Cli, Command, ConfigCommand, NotificationsCommand, SunsetCommand,
    WeatherCommand,
};
use errors::Exit;
use glimpse_utils::init_app_tracing;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
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
        Command::Sunset(SunsetCommand::Mode { mode }) => commands::sunset_mode(bus()?, mode).await,
        Command::Weather(WeatherCommand::Status) => {
            commands::weather_status(bus()?, cli.json).await
        }
        Command::Weather(WeatherCommand::Refresh) => commands::weather_refresh(bus()?).await,
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
        Command::Config(ConfigCommand::Show) => commands::config_show(cli.config.config, cli.json),
        Command::Config(ConfigCommand::Validate { path }) => {
            commands::config_validate(path.or(cli.config.config))
        }
        Command::Config(ConfigCommand::Path) => commands::config_path(cli.config.config, cli.json),
        Command::Applets(AppletsCommand::List) => {
            commands::applets_list(cli.config.config, cli.json).await
        }
        Command::Applets(AppletsCommand::New(args)) => commands::applets_new(args),
        Command::Applets(AppletsCommand::Check { dir }) => commands::applets_check(&dir).await,
        Command::Applets(AppletsCommand::Dev { dir }) => {
            commands::applets_dev(&dir, cli.config.config).await
        }
        Command::Applets(AppletsCommand::Bundle { dir, prefix, out }) => {
            commands::applets_bundle(&dir, &prefix, &out).await
        }
        Command::Applets(AppletsCommand::Install { dir }) => commands::applets_install(&dir).await,
        Command::Applets(AppletsCommand::Uninstall { id }) => commands::applets_uninstall(&id),
        Command::Applets(AppletsCommand::Inspect { id }) => {
            commands::applets_inspect(cli.config.config, id, cli.json).await
        }
        Command::Applets(AppletsCommand::Restart { id }) => commands::applets_restart(id).await,
        Command::Doctor => commands::doctor(cli.config.config, cli.json).await,
    }
}
