use std::ffi::OsStr;
use std::path::PathBuf;

use clap::{Parser, ValueEnum};

// clap wants a `&'static str`, so the protocol number is literal; the assertion catches drift.
const LONG_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (protocol 1)");
const _: () = assert!(
    glimpse_proto::PROTOCOL_VERSION == 1,
    "PROTOCOL_VERSION changed — update LONG_VERSION to match"
);

#[derive(Debug, Parser)]
#[command(
    name = "glimpsed",
    about = "The glimpse session daemon.",
    version = LONG_VERSION
)]
pub struct Cli {
    #[arg(
        short,
        long,
        value_name = "PATH",
        env = "GLIMPSE_CONFIG_PATH",
        help = "Use exactly this file, skipping the system and user layers"
    )]
    pub config: Option<PathBuf>,

    #[arg(
        long,
        value_name = "PATH",
        env = "GLIMPSE_SOCKET_PATH",
        help = "Override the listening socket"
    )]
    pub socket: Option<PathBuf>,

    #[arg(long, help = "Load and validate configuration, print problems, exit")]
    pub check_config: bool,

    #[arg(long, help = "Print the merged configuration as TOML and exit")]
    pub print_config: bool,

    #[arg(
        long,
        value_name = "SERVICES",
        value_delimiter = ',',
        value_parser = service_name,
        conflicts_with = "without",
        help = "Comma-separated allowlist; every other service stays unregistered"
    )]
    pub only: Vec<String>,

    #[arg(
        long,
        value_name = "SERVICES",
        value_delimiter = ',',
        value_parser = service_name,
        help = "Comma-separated denylist"
    )]
    pub without: Vec<String>,

    #[arg(
        long,
        value_name = "FILTER",
        env = "RUST_LOG",
        default_value = "info",
        help = "tracing-subscriber filter, same syntax as RUST_LOG"
    )]
    pub log: String,

    #[arg(
        long,
        value_name = "FMT",
        value_enum,
        default_value_t = LogFormat::Auto,
        help = "auto drops timestamps and colour under a journal stream"
    )]
    pub log_format: LogFormat,
}

// `--only ''` would otherwise read as "no filter" and start every service.
fn service_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("service name is empty".into());
    }
    Ok(name.to_owned())
}

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
    // The journal stamps and colours its own lines, so ours arrive doubled.
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn version_names_the_protocol() {
        assert!(LONG_VERSION.contains("(protocol 1)"), "{LONG_VERSION}");
    }

    #[test]
    fn only_and_without_are_mutually_exclusive() {
        let error = Cli::try_parse_from(["glimpsed", "--only", "audio", "--without", "tray"])
            .expect_err("supplying both should be a usage error");
        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn service_lists_split_on_commas() {
        let cli = Cli::try_parse_from(["glimpsed", "--only", "audio,network"])
            .expect("arguments should parse");
        assert_eq!(cli.only, ["audio", "network"]);
    }

    #[test]
    fn a_blank_service_name_is_rejected() {
        Cli::try_parse_from(["glimpsed", "--only", ""])
            .expect_err("an empty service name should not read as no filter");
    }

    #[test]
    fn auto_drops_timestamps_only_under_a_journal_stream() {
        assert_eq!(
            LogFormat::Auto.resolve(Some(OsStr::new("8:12345"))),
            LogSink::Journal
        );
        assert_eq!(LogFormat::Auto.resolve(None), LogSink::Terminal);
        assert_eq!(
            LogFormat::Auto.resolve(Some(OsStr::new(""))),
            LogSink::Terminal
        );
    }

    #[test]
    fn explicit_log_formats_ignore_the_journal() {
        let stream = Some(OsStr::new("8:12345"));
        assert_eq!(LogFormat::Plain.resolve(stream), LogSink::Terminal);
        assert_eq!(LogFormat::Json.resolve(stream), LogSink::Json);
    }
}
