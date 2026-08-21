mod cli;

use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use cli::{Cli, Command, ConfigCommand};

mod exit {
    pub const COMMAND_FAILED: u8 = 1;
    pub const DAEMON_UNREACHABLE: u8 = 3;
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--socket` and `GLIMPSE_SOCKET_PATH` name the socket outright, so only the default needs a
    // runtime directory.
    let socket = if !cli.command.needs_daemon() {
        None
    } else if let Some(socket) = glimpse_ipc::socket_path() {
        Some(socket)
    } else {
        eprintln!("glimpsectl: XDG_RUNTIME_DIR is unset, so there is no socket to reach");
        return exit::DAEMON_UNREACHABLE.into();
    };

    if cli.no_color {
        anstream::ColorChoice::Never.write_global();
    }
    let color = anstream::AutoStream::choice(&std::io::stdout());
    let format = if cli.json { "json" } else { "text" };

    eprintln!(
        "glimpsectl: not implemented yet: {}",
        invocation(&cli.command)
    );
    if let Some(socket) = socket {
        eprintln!("  socket   {}", socket.display());
        eprintln!("  timeout  {:?}", Duration::from_millis(cli.timeout));
    }
    eprintln!("  output   {format}, color {color:?}");

    // Not success: nothing reached stdout, and a script must not read "did nothing" as "worked".
    exit::COMMAND_FAILED.into()
}

// Each arm is where that subcommand's implementation goes; for now it renders what it parsed, so
// the argument surface can be exercised before anything speaks to a socket.
fn invocation(command: &Command) -> String {
    match command {
        Command::Get { topic, field } => match field {
            Some(field) => format!("get {topic} --field {field}"),
            None => format!("get {topic}"),
        },
        Command::Watch { pattern, count } => match count {
            Some(count) => format!("watch {pattern} --count {count}"),
            None => format!("watch {pattern}"),
        },
        Command::Call { method, arguments } => {
            let arguments: Vec<_> = arguments
                .iter()
                .map(|(key, value)| format!(" {key}={value}"))
                .collect();
            format!("call {method}{}", arguments.concat())
        }
        Command::Topics { pattern } => match pattern {
            Some(pattern) => format!("topics {pattern}"),
            None => "topics".to_owned(),
        },
        Command::Services => "services".to_owned(),
        Command::Config(ConfigCommand::Show) => "config show".to_owned(),
        Command::Config(ConfigCommand::Validate { path }) => match path {
            Some(path) => format!("config validate {}", path.display()),
            None => "config validate".to_owned(),
        },
        Command::Config(ConfigCommand::Path) => "config path".to_owned(),
        Command::Doctor => "doctor".to_owned(),
        Command::Monitor => "monitor".to_owned(),
    }
}
