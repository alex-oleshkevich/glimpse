use shadow_rs::shadow;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpsectl",
    about = "Read topics, invoke commands and inspect the glimpse daemon.",
    version = build::VERSION
)]
pub struct Cli {
    #[command(flatten)]
    pub socket: glimpse_utils::SocketArg,

    #[command(flatten)]
    pub config: glimpse_utils::ConfigArg,

    #[command(flatten)]
    pub log: glimpse_utils::LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Print the current value of one topic")]
    Get {
        #[arg(value_name = "TOPIC", help = "Exact topic name")]
        topic: String,

        #[arg(long, value_name = "PATH", help = "Print one field of the payload")]
        field: Option<String>,
    },

    #[command(about = "Print the snapshot then every update, one per line")]
    Watch {
        #[arg(value_name = "PATTERN", help = "Topic pattern, `audio.*` or `tray.**`")]
        pattern: String,

        #[arg(long, value_name = "N", help = "Exit after N events")]
        count: Option<u64>,
    },

    #[command(about = "Invoke a command and print the result")]
    Call {
        #[arg(value_name = "METHOD", help = "Command name, `audio.set_volume`")]
        method: String,

        #[arg(
            value_name = "KEY=VALUE",
            value_parser = key_value,
            help = "Arguments; a value parses as JSON when it can, otherwise as a string"
        )]
        arguments: Vec<(String, String)>,
    },

    #[command(about = "List known topics with their owning service")]
    Topics {
        #[arg(value_name = "PATTERN", help = "Only topics matching this pattern")]
        pattern: Option<String>,
    },

    #[command(about = "List services with state, health and the reason for `degraded`")]
    Services,

    #[command(subcommand, about = "Inspect the configuration stack")]
    Config(ConfigCommand),

    #[command(about = "Check socket, compositor, Wayland protocols, session bus and backends")]
    Doctor,

    #[command(about = "Interactive TUI: topic browser, live values, service health")]
    Monitor,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    #[command(about = "Print the daemon's merged configuration")]
    Show,

    #[command(about = "Validate a file, or the layered stack, and report where a problem is")]
    Validate {
        #[arg(value_name = "PATH", help = "Validate this file instead of the stack")]
        path: Option<PathBuf>,
    },

    #[command(about = "Print the files the layered stack resolved to, in order")]
    Path,
}

impl Command {
    pub fn needs_daemon(&self) -> bool {
        !matches!(self, Self::Config(_))
    }
}

// Split on the first `=` only: a JSON value carries its own, as in `where={"x":1}`.
fn key_value(raw: &str) -> Result<(String, String), String> {
    let Some((key, value)) = raw.split_once('=') else {
        return Err(format!("expected KEY=VALUE, got {raw:?}"));
    };
    if key.is_empty() {
        return Err("argument name is empty".into());
    }
    Ok((key.to_owned(), value.to_owned()))
}
