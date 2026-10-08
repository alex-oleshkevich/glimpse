use clap::Parser;
use glimpse_utils::{ConfigArg, LogArgs};

#[derive(Debug, Parser)]
#[command(about = "Glimpse desktop settings provider.", version)]
pub struct Cli {
    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}
