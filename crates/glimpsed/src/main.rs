mod cli;

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, LogSink};

mod exit {
    pub const OK: u8 = 0;
    pub const NO_RUNTIME_DIR: u8 = 4;
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    init_tracing(
        &cli.log,
        cli.log_format
            .resolve(std::env::var_os("JOURNAL_STREAM").as_deref()),
    );

    let Some(runtime_dir) = dirs::runtime_dir() else {
        tracing::error!("XDG_RUNTIME_DIR is unset or is not an absolute path");
        return exit::NO_RUNTIME_DIR.into();
    };
    // The daemon binds rather than discovers: a socket that is already there means another
    // daemon may own it, which is a refusal to start and not the path to use.
    let socket = cli
        .socket
        .clone()
        .unwrap_or_else(|| runtime_dir.join(glimpse_ipc::SOCKET_RELATIVE_PATH));

    run(&cli, &socket)
}

fn run(cli: &Cli, socket: &Path) -> ExitCode {
    if !cli.only.is_empty() {
        tracing::info!(names = ?cli.only, "running only these services");
    } else if !cli.without.is_empty() {
        tracing::info!(names = ?cli.without, "running without these services");
    }

    let config = match &cli.config {
        Some(path) => path.display().to_string(),
        None => "the layered stack".to_owned(),
    };

    tracing::info!(socket = %socket.display(), %config, "glimpsed {}", env!("CARGO_PKG_VERSION"));
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
