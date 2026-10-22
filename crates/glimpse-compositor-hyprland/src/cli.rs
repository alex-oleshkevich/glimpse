use clap::Parser;
use glimpse_utils::LogArgs;

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-compositor-hyprland",
    about = "Compositor provider for Hyprland.",
    version
)]
pub struct Cli {
    #[arg(long, help = "Exit 0 if Hyprland answers, 1 otherwise.")]
    pub probe: bool,

    #[command(flatten)]
    pub log: LogArgs,
}
