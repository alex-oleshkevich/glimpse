use std::path::{Path, PathBuf};

use crate::log::LogFormat;

#[allow(clippy::exhaustive_structs)]
#[derive(Clone, Default, Debug, PartialEq, Eq, clap::Args)]
#[command(about=None, long_about=None)]
pub struct LogArgs {
    #[arg(
        long,
        value_enum,
        global = true,
        value_name = "FORMAT",
        default_value_t = LogFormat::Plain,
        help = "Select the log output format (plain text or JSON)."
    )]
    pub log_format: LogFormat,

    #[arg(
        long,
        global = true,
        env = "RUST_LOG",
        value_name = "LEVEL",
        default_value = "info",
        help = "Set the logging filter (for example, info, debug, or trace)."
    )]
    pub log: String,
}

#[allow(clippy::exhaustive_structs)]
#[derive(Clone, Default, Debug, PartialEq, Eq, clap::Args)]
#[command(about=None, long_about=None)]
pub struct ConfigArg {
    #[arg(
        short,
        long,
        global = true,
        env = "GLIMPSE_CONFIG_PATH",
        value_name = "PATH",
        help = "Path to the configuration file."
    )]
    pub config: Option<PathBuf>,
}

impl ConfigArg {
    pub fn as_deref(&self) -> Option<&Path> {
        self.config.as_deref()
    }
}

#[allow(clippy::exhaustive_structs)]
#[derive(Clone, Default, Debug, PartialEq, Eq, clap::Args)]
#[command(about=None, long_about=None)]
pub struct SocketArg {
    #[arg(
        short,
        long,
        global = true,
        env = "GLIMPSED_SOCKET_PATH",
        value_name = "PATH",
        help = "Path to the glimpsed socket."
    )]
    pub socket: Option<PathBuf>,
}

impl SocketArg {
    pub fn as_deref(&self) -> Option<&Path> {
        self.socket.as_deref()
    }
}
