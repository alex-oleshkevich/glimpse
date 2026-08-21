# Four-file skeleton

A complete starting point for a new binary crate. Delete what the binary does not need. The rules
behind every line are in `SKILL.md`; this file is the shape, not the reasoning.

Manifest (when using workspaces):

```toml
[dependencies]
anstream.workspace = true
anyhow.workspace = true
clap.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

## `src/main.rs`

```rust
mod cli;
mod commands;
mod errors;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, ConfigCommand, LogSink};
use errors::Exit;

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
        Ok(()) => Exit::Ok.into(),
        Err(error) => {
            // A failure is the answer, not a diagnostic, so it must not depend on --log.
            anstream::eprintln!("mytool: {error:#}");
            errors::exit(&error).into()
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let json = cli.json;

    match cli.command {
        Command::Get { key, field } => commands::get(key, field, json).await,
        Command::Watch { pattern, count } => commands::watch(pattern, count, json).await,
        Command::Config(ConfigCommand::Show) => commands::config_show(json),
        Command::Config(ConfigCommand::Path) => commands::config_path(cli.config, json),
    }
}

// The filter also comes from RUST_LOG, which is inherited: a stale value in somebody's profile
// must not stop the program.
fn init_tracing(filter: &str, sink: LogSink, color: bool) {
    let env_filter = match tracing_subscriber::EnvFilter::try_new(filter) {
        Ok(env_filter) => env_filter,
        Err(error) => {
            eprintln!("mytool: ignoring invalid log filter {filter:?}: {error}");
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
```

## `src/cli.rs`

```rust
use std::{ffi::OsStr, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum LogFormat {
    Auto,
    Plain,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSink {
    Terminal,
    // The journal stamps and colors its own lines, so ours arrive doubled.
    Journal,
    Json,
}

impl LogFormat {
    // An empty variable means unset; units and shells both export empty values.
    pub fn resolve(self, journal_stream: Option<&OsStr>) -> LogSink {
        let under_journal = journal_stream.is_some_and(|value| !value.is_empty());
        match self {
            Self::Json => LogSink::Json,
            Self::Plain => LogSink::Terminal,
            Self::Auto if under_journal => LogSink::Journal,
            Self::Auto => LogSink::Terminal,
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "mytool", about = "One line, no trailing period.")]
pub struct Cli {
    #[arg(
        short,
        long,
        value_name = "PATH",
        env = "MYTOOL_CONFIG_PATH",
        global = true,
        help = "Use exactly this file, skipping the layered stack"
    )]
    pub config: Option<PathBuf>,

    #[arg(
        short,
        long,
        global = true,
        help = "Emit raw JSON instead of formatted output"
    )]
    pub json: bool,

    #[arg(long, global = true, help = "Disable color; NO_COLOR does the same")]
    pub no_color: bool,

    #[arg(
        long,
        value_name = "FILTER",
        env = "RUST_LOG",
        default_value = "info",
        global = true,
        help = "tracing-subscriber filter, same syntax as RUST_LOG"
    )]
    pub log: String,

    #[arg(
        long,
        value_name = "FMT",
        value_enum,
        default_value_t = LogFormat::Auto,
        global = true,
        help = "auto drops timestamps and color under a journal stream"
    )]
    pub log_format: LogFormat,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Print the current value of one key")]
    Get {
        #[arg(value_name = "KEY", help = "Exact key name")]
        key: String,

        #[arg(long, value_name = "PATH", help = "Print one field of the payload")]
        field: Option<String>,
    },

    #[command(about = "Print the snapshot then every update, one per line")]
    Watch {
        #[arg(value_name = "PATTERN", help = "Key pattern, `server.*`")]
        pattern: String,

        #[arg(long, value_name = "N", help = "Exit after N events")]
        count: Option<u64>,
    },

    #[command(subcommand, about = "Inspect the configuration stack")]
    Config(ConfigCommand),
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    #[command(about = "Print the merged configuration")]
    Show,

    #[command(about = "Print the files the stack resolved to, in order")]
    Path,
}

impl Command {
    // `config path` reads the stack from disk, so it answers with nothing running — which is the
    // case a broken configuration produces.
    pub fn needs_backend(&self) -> bool {
        !matches!(self, Self::Config(ConfigCommand::Path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn a_missing_subcommand_is_a_usage_error() {
        let error = Cli::try_parse_from(["mytool"]).expect_err("a subcommand is required");
        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn global_flags_are_accepted_after_the_subcommand() {
        let cli = Cli::try_parse_from(["mytool", "get", "server.port", "--json"])
            .expect("arguments should parse");
        assert!(cli.json);
    }
}
```

### A `KEY=VALUE` value parser

For a subcommand taking free-form arguments. Splits on the first `=` only, so a JSON value carrying
its own survives.

```rust
#[arg(
    value_name = "KEY=VALUE",
    value_parser = key_value,
    help = "Arguments; a value parses as JSON when it can, otherwise as a string"
)]
arguments: Vec<(String, String)>,

fn key_value(raw: &str) -> Result<(String, String), String> {
    let Some((key, value)) = raw.split_once('=') else {
        return Err(format!("expected KEY=VALUE, got {raw:?}"));
    };
    if key.is_empty() {
        return Err("argument name is empty".into());
    }
    Ok((key.to_owned(), value.to_owned()))
}
```

### A version string carrying build metadata

Guard it with a const assertion, so the number cannot drift from what it claims:

```rust
const LONG_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (protocol 1)");
const _: () = assert!(
    mylib::PROTOCOL_VERSION == 1,
    "PROTOCOL_VERSION changed — update LONG_VERSION to match"
);

#[command(version = LONG_VERSION)]
```

## `src/commands.rs`

```rust
use std::path::PathBuf;

use anyhow::{Result, bail};

pub async fn get(_key: String, _field: Option<String>, _json: bool) -> Result<()> {
    bail!("get is not implemented yet")
}

pub async fn watch(_pattern: String, _count: Option<u64>, _json: bool) -> Result<()> {
    bail!("watch is not implemented yet")
}

pub fn config_show(_json: bool) -> Result<()> {
    bail!("config show is not implemented yet")
}

pub fn config_path(_config: Option<PathBuf>, _json: bool) -> Result<()> {
    bail!("config path is not implemented yet")
}
```

### Streaming output that survives a closed pipe

```rust
use std::io::{ErrorKind, Write};

pub async fn watch(pattern: String, count: Option<u64>, json: bool) -> Result<()> {
    let mut out = anstream::stdout().lock();
    for event in stream {
        // `mytool watch … | head -1` closes the pipe; that is a clean exit, not a failure.
        if let Err(error) = writeln!(out, "{}", render(&event, json))
            && error.kind() == ErrorKind::BrokenPipe
        {
            return Ok(());
        }
    }
    Ok(())
}
```

## `src/errors.rs`

```rust
use std::process::ExitCode;

/// Every code this binary is specified to return. 2 is absent because clap owns usage errors and
/// exits before `main` runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Unreachable = 3,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

pub fn exit(error: &anyhow::Error) -> Exit {
    match error.is::<mylib::Unreachable>() {
        true => Exit::Unreachable,
        false => Exit::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_failure_is_one() {
        assert_eq!(exit(&anyhow::anyhow!("something went wrong")), Exit::Failed);
    }

    // The whole reason the mapping lives in one place: context added anywhere upstream must not
    // change which code a script sees.
    #[test]
    fn a_context_layer_does_not_change_the_code() {
        let error = anyhow::Error::new(mylib::Unreachable).context("reading server.port");
        assert_eq!(exit(&error), Exit::Unreachable);
    }
}
```

## A marker error, when `main` must branch on something the binary itself detects

The one place `thiserror` belongs inside a binary:

```rust
#[derive(Debug, thiserror::Error)]
#[error("cannot reach the service at {0}")]
pub struct Unreachable(pub PathBuf);
```

Return it with `bail!(Unreachable(path))`, recover it with `error.downcast_ref::<Unreachable>()`.
