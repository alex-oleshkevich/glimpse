use clap::{CommandFactory as _, Parser, ValueEnum, error::ErrorKind};
use glimpse_config::ColorFormat;
use glimpse_utils::LogArgs;
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

    #[arg(long, value_parser = format, value_name = "FORMAT", help = "Print the picked color in this notation instead of hex; requires --tool picker.")]
    pub format: Option<ColorFormat>,

    #[arg(
        long,
        help = "Print the picked color as one JSON object; requires --tool picker."
    )]
    pub json: bool,

    #[arg(
        long,
        help = "Copy the result to the clipboard through the panel; requires --tool."
    )]
    pub copy: bool,

    #[arg(
        long,
        default_value_t = 180,
        value_name = "PIXELS",
        help = "The lens radius in logical pixels."
    )]
    pub radius: u32,

    #[arg(
        long,
        default_value_t = 3,
        value_name = "TIMES",
        help = "The initial magnification."
    )]
    pub zoom: u32,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}

impl Cli {
    pub fn clamped_radius(&self) -> f32 {
        self.radius.clamp(crate::MIN_RADIUS, crate::MAX_RADIUS) as f32
    }

    pub fn clamped_zoom(&self) -> u32 {
        self.zoom.clamp(crate::MIN_ZOOM, crate::MAX_ZOOM)
    }

    pub fn validate(&self) -> Result<(), clap::Error> {
        if self.tool != Some(Tool::Picker) && (self.format.is_some() || self.json) {
            return Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "--format and --json require --tool picker",
            ));
        }
        if self.copy && self.tool.is_none() {
            return Err(
                Self::command().error(ErrorKind::ArgumentConflict, "--copy requires --tool")
            );
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
