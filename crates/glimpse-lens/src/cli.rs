use clap::{CommandFactory as _, Parser, ValueEnum, error::ErrorKind};
use glimpse_config::ColorFormat;
use glimpse_utils::{ConfigArg, LogArgs};
use shadow_rs::shadow;

shadow!(build);

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Tool {
    Picker,
    Ruler,
}

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-lens",
    about = "Magnify the frozen screen, pick a color or measure distances.",
    version = build::VERSION
)]
pub struct Cli {
    #[arg(
        long,
        value_enum,
        help = "Pick a color or measure distances instead of magnifying only."
    )]
    pub tool: Option<Tool>,

    #[arg(long, value_parser = format, value_name = "FORMAT", help = "Override [color-picker] format; requires --tool picker.")]
    pub format: Option<ColorFormat>,

    #[arg(
        long,
        help = "Print the picked color as one JSON object; requires --tool picker."
    )]
    pub json: bool,

    #[arg(
        long,
        value_name = "PIXELS",
        help = "Override the selected tool's configured lens radius in logical pixels."
    )]
    pub lens_radius: Option<u32>,

    #[arg(
        long,
        value_name = "TIMES",
        help = "Override the selected tool's configured maximum magnification."
    )]
    pub max_zoom: Option<u32>,

    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}

impl Cli {
    pub fn validate(&self) -> Result<(), clap::Error> {
        if self.tool != Some(Tool::Picker) && (self.format.is_some() || self.json) {
            return Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "--format and --json require --tool picker",
            ));
        }
        Ok(())
    }
}

fn format(name: &str) -> Result<ColorFormat, String> {
    ColorFormat::parse(name)
        .ok_or_else(|| format!("expected hex, rgb, hsl, hsv, oklch or cmyk, got {name:?}"))
}

#[cfg(test)]
mod tests;
