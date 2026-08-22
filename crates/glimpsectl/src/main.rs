mod cli;
mod commands;
use anyhow::{Context, Result};

use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Command, ConfigCommand};

use crate::cli::LogSink;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // Before the subscriber exists: a terminal sink fixes its ANSI setting when it is built.
    if cli.no_color {
        anstream::ColorChoice::Never.write_global();
    }
    // tracing writes to stderr, so stderr is the stream whose color support decides.
    let color = anstream::AutoStream::choice(&std::io::stderr()) != anstream::ColorChoice::Never;

    init_tracing(
        &cli.log,
        cli.log_format
            .resolve(std::env::var_os("JOURNAL_STREAM").as_deref()),
        color,
    );

    match run(cli).await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
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
    let json = cli.json;

    match cli.command {
        Command::Get { topic, field } => commands::get(daemon()?, topic, field, json).await,
        Command::Watch { pattern, count } => commands::watch(daemon()?, pattern, count, json).await,
        Command::Call { method, arguments } => {
            commands::call(daemon()?, method, arguments, json).await
        }
        Command::Topics { pattern } => commands::topics(daemon()?, pattern, json).await,
        Command::Services => commands::services(daemon()?, json).await,
        Command::Config(ConfigCommand::Show) => commands::config_show(cli.config, json),
        Command::Config(ConfigCommand::Validate { path }) => {
            commands::config_validate(path.or(cli.config), json)
        }
        Command::Config(ConfigCommand::Path) => commands::config_path(cli.config, json),
        Command::Doctor => commands::doctor(daemon()?, json).await,
        Command::Monitor => commands::monitor(daemon()?).await,
    }
}

fn init_tracing(filter: &str, sink: LogSink, color: bool) {
    let env_filter = match tracing_subscriber::EnvFilter::try_new(filter) {
        Ok(env_filter) => env_filter,
        Err(error) => {
            eprintln!("glimpsectl: ignoring invalid log filter {filter:?}: {error}");
            tracing_subscriber::EnvFilter::new("info")
        }
    };

    let builder = tracing_subscriber::fmt().with_env_filter(env_filter);
    match sink {
        LogSink::Terminal => builder.with_ansi(color).init(),
        LogSink::Journal => builder.without_time().with_ansi(false).init(),
        LogSink::Json => builder.json().init(),
    }
}
