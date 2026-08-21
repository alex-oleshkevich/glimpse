mod cli;
mod errors;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, LogSink};
use glimpse_config::Config;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    init_tracing(
        &cli.log,
        cli.log_format
            .resolve(std::env::var_os("JOURNAL_STREAM").as_deref()),
    );

    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error:#}");
            errors::exit_code(&error)
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    if !cli.only.is_empty() {
        tracing::info!(names = ?cli.only, "running only these services");
    } else if !cli.without.is_empty() {
        tracing::info!(names = ?cli.without, "running without these services");
    }

    let config = Config::load(&cli.config);
    for path in config.get_loaded_files() {
        tracing::info!(path = %path.display(), "loaded config file");
    }

    let socket = glimpse_ipc::socket_path(cli.socket.as_deref())?;
    let _listener = glimpse_ipc::Server::listen(&socket).await?;
    tracing::info!(socket = %socket.display(), "glimpsed {}", env!("CARGO_PKG_VERSION"));
    tracing::warn!("the broker, service registry and socket server are not implemented yet");

    Ok(())
}

// The filter also comes from RUST_LOG, which is inherited: a stale value in someone's profile
// must not stop the session daemon.
fn init_tracing(filter: &str, sink: LogSink) {
    let env_filter = match tracing_subscriber::EnvFilter::try_new(filter) {
        Ok(env_filter) => env_filter,
        Err(error) => {
            eprintln!("glimpsed: ignoring invalid log filter {filter:?}: {error}");
            tracing_subscriber::EnvFilter::new("info")
        }
    };

    let builder = tracing_subscriber::fmt().with_env_filter(env_filter);
    match sink {
        LogSink::Terminal => builder.init(),
        LogSink::Journal => builder.without_time().with_ansi(false).init(),
        LogSink::Json => builder.json().init(),
    }
}
