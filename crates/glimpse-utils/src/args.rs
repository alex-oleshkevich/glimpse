use std::path::PathBuf;

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
        long,
        global = true,
        env = "GLIMPSE_CONFIG_DIR",
        value_name = "DIR",
        value_parser = clap::builder::TypedValueParser::map(
            clap::builder::OsStringValueParser::new(),
            PathBuf::from
        ),
        help = "Directory stacked above the user configuration."
    )]
    pub config_dir: Option<PathBuf>,
}

#[cfg(test)]
mod tests;
