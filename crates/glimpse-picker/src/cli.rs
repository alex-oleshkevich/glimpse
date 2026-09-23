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
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn the_argument_surface_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_flag_is_optional() {
        let cli = Cli::try_parse_from(["glimpse-picker"]).unwrap();

        assert!(cli.format.is_none());
        assert!(!cli.json);
        assert!(cli.lens_radius.is_none());
        assert!(cli.max_zoom.is_none());
    }

    #[test]
    fn a_format_is_taken_by_name() {
        let cli = Cli::try_parse_from([
            "glimpse-picker",
            "--format",
            "oklch",
            "--json",
            "--lens-radius",
            "120",
            "--max-zoom",
            "12",
        ])
        .unwrap();

        assert_eq!(cli.format, Some(ColorFormat::Oklch));
        assert!(cli.json);
        assert_eq!(cli.lens_radius, Some(120));
        assert_eq!(cli.max_zoom, Some(12));
    }

    #[test]
    fn an_unknown_format_is_a_usage_error() {
        let error = Cli::try_parse_from(["glimpse-picker", "--format", "lab"]).unwrap_err();

        assert_eq!(error.exit_code(), 2);
    }
}
