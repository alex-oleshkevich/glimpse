use clap::Parser;
use glimpse_config::ColorFormat;
use glimpse_utils::{ConfigArg, LogArgs};
use shadow_rs::shadow;

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-picker",
    about = "Pick a color from the screen through a zoom lens and print it on stdout.",
    version = build::VERSION
)]
pub struct Cli {
    #[arg(
        long,
        value_parser = format,
        value_name = "FORMAT",
        help = "Print in this notation instead of [color-picker] format: hex, rgb, hsl, hsv, oklch or cmyk."
    )]
    pub format: Option<ColorFormat>,

    #[arg(
        long,
        help = "Print one JSON object with the notation, the rendered value and the red, green and blue channels."
    )]
    pub json: bool,

    #[arg(
        long,
        value_name = "PIXELS",
        help = "The lens radius in logical pixels, instead of [color-picker] lens-radius."
    )]
    pub lens_radius: Option<u32>,

    #[arg(
        long,
        value_name = "TIMES",
        help = "How far the scroll wheel zooms in, instead of [color-picker] max-zoom."
    )]
    pub max_zoom: Option<u32>,

    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}

fn format(name: &str) -> Result<ColorFormat, String> {
    ColorFormat::parse(name)
        .ok_or_else(|| format!("expected hex, rgb, hsl, hsv, oklch or cmyk, got {name:?}"))
}

#[cfg(test)]
mod tests;
