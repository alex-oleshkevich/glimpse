mod cli;

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, LogSink};
use glimpse_config::Config;

mod exit {
    pub const OK: u8 = 0;
    pub const NO_DAEMON: u8 = 1;
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    init_tracing(
        &cli.log,
        cli.log_format
            .resolve(std::env::var_os("JOURNAL_STREAM").as_deref()),
    );

    let Some(socket) = glimpse_ipc::socket_path() else {
        tracing::error!("daemon socket not found, is it running?");
        return exit::NO_DAEMON.into();
    };

    if !cli.only.is_empty() {
        tracing::info!(names = ?cli.only, "running only these services");
    } else if !cli.without.is_empty() {
        tracing::info!(names = ?cli.without, "running without these services");
    }

    let config = Config::load(&cli.config);
    config.get_loaded_files().iter().for_each(|p| {
        tracing::info!(path = %p.display(), "loaded config file");
    });

    run(&config, &socket).await
}

async fn run(_config: &Config, _socket: &Path) -> ExitCode {
    // tracing::info!(socket = %socket.display(), %config, "glimpsed {}", env!("CARGO_PKG_VERSION"));
    tracing::warn!("the broker, socket server and service registry are not implemented yet");
    exit::OK.into()
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
