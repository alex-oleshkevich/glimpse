use clap::Parser;
use glimpse_utils::{ConfigArg, LogArgs, SocketArg};
use shadow_rs::shadow;

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-sunset",
    about = "Night light service.",
    version = build::VERSION
)]
pub struct Cli {
    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub socket: SocketArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}
