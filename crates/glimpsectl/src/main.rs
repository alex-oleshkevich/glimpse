mod cli;
mod commands;
use anyhow::{Context, Result};
use glimpse_utils::init_app_tracing;

use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Command, ConfigCommand};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();

    match run(cli).await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    init_app_tracing(&cli.log.log, cli.log.log_format);

    let client = match cli.command.needs_daemon() {
        true => {
            let socket = glimpse_ipc::socket_path(cli.socket.as_deref())?;
            Some(glimpse_ipc::Client::connect(&socket).await?)
        }
        false => None,
    };

    let daemon = || {
        client
            .as_ref()
            .context("this command needs a running daemon")
    };

    match cli.command {
        Command::Get { topic, field } => commands::get(daemon()?, topic, field).await,
        Command::Watch { pattern, count } => commands::watch(daemon()?, pattern, count).await,
        Command::Call { method, arguments } => commands::call(daemon()?, method, arguments).await,
        Command::Topics { pattern } => commands::topics(daemon()?, pattern).await,
        Command::Services => commands::services(daemon()?).await,
        Command::Config(ConfigCommand::Show) => commands::config_show(cli.config.config),
        Command::Config(ConfigCommand::Validate { path }) => {
            commands::config_validate(path.or(cli.config.config))
        }
        Command::Config(ConfigCommand::Path) => commands::config_path(cli.config.config),
        Command::Doctor => commands::doctor(daemon()?).await,
        Command::Monitor => commands::monitor(daemon()?).await,
    }
}
