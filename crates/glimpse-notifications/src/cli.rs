use clap::Parser;
use glimpse_utils::{ConfigArg, LogArgs};
use shadow_rs::shadow;

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-notifications",
    about = "Glimpse notification popup service.",
    version = build::VERSION
)]
pub struct Cli {
    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}
