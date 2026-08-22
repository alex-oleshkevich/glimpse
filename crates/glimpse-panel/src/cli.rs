use shadow_rs::shadow;

use clap::Parser;

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-panel",
    about,
    version = build::VERSION
)]
pub struct Cli {
    #[command(flatten)]
    pub socket: glimpse_utils::args::SocketArg,

    #[command(flatten)]
    pub config: glimpse_utils::args::ConfigArg,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,

    #[command(flatten)]
    pub log: glimpse_utils::args::LogArgs,
}
