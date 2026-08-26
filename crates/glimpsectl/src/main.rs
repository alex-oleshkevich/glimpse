mod cli;
mod commands;
mod errors;
mod render;

use std::{process::ExitCode, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Command, ConfigCommand};
use commands::Session;
use errors::Exit;
use glimpse_utils::init_app_tracing;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    cli.color.write_global();
    init_app_tracing(&cli.log.log, cli.log.log_format);

    match run(cli).await {
        Ok(()) => Exit::Ok.into(),
        Err(error) => {
            // A failure is the answer, not a log line: routing it through the filter would make
            // `--log warn` exit non-zero with nothing said. `{error:#}` flattens the whole chain.
            anstream::eprintln!("glimpsectl: {error:#}");
            errors::exit(&error).into()
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let session = match cli.command.needs_daemon() {
        true => {
            let socket = glimpse_ipc::socket_path(cli.socket.as_deref())?;
            let client =
                glimpse_ipc::Client::connect(&socket, Duration::from_millis(cli.timeout)).await?;
            Some(Session { client })
        }
        false => None,
    };

    let daemon = || {
        session
            .as_ref()
            .context("this command needs a running daemon")
    };

    match cli.command {
        Command::Get { topic, field, json } => commands::get(daemon()?, topic, field, json).await,
        Command::Watch {
            pattern,
            count,
            json,
        } => commands::watch(daemon()?, pattern, count, json).await,
        Command::Call { method, arguments } => commands::call(daemon()?, method, arguments).await,
        Command::Topics { pattern, owner } => commands::topics(daemon()?, pattern, owner).await,
        Command::Methods { pattern, owner } => commands::methods(daemon()?, pattern, owner).await,
        Command::Services => commands::services(daemon()?).await,
        Command::Config(ConfigCommand::Show) => commands::config_show(cli.config.config),
        Command::Config(ConfigCommand::Validate { path }) => {
            commands::config_validate(path.or(cli.config.config))
        }
        Command::Config(ConfigCommand::Path) => commands::config_path(cli.config.config),
        Command::Doctor => commands::doctor(cli.socket, cli.config.config).await,
        Command::Monitor => commands::monitor(daemon()?).await,
    }
}
