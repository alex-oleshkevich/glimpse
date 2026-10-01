use clap::Parser;
use glimpse_utils::{ConfigArg, LogArgs};
use shadow_rs::shadow;

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-ruler",
    about = "Measure distances on the screen through a zoom lens and print each segment on stdout as one JSON line.",
    version = build::VERSION
)]
pub struct Cli {
    #[arg(
        long,
        value_name = "PIXELS",
        help = "The lens radius in logical pixels, instead of [ruler] lens-radius."
    )]
    pub lens_radius: Option<u32>,

    #[arg(
        long,
        value_name = "TIMES",
        help = "How far the scroll wheel zooms in, instead of [ruler] max-zoom."
    )]
    pub max_zoom: Option<u32>,

    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}

#[cfg(test)]
mod tests;
