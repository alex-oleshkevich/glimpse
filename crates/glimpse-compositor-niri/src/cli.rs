use clap::Parser;
use glimpse_utils::LogArgs;

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-compositor-niri",
    about = "Compositor provider for niri.",
    version
)]
pub struct Cli {
    #[arg(long, help = "Exit 0 if niri answers a version request, 1 otherwise.")]
    pub probe: bool,

    #[command(flatten)]
    pub log: LogArgs,
}
