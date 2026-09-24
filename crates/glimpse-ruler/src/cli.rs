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
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn the_argument_surface_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_flag_is_optional() {
        let cli = Cli::try_parse_from(["glimpse-ruler"]).unwrap();

        assert!(cli.lens_radius.is_none());
        assert!(cli.max_zoom.is_none());
    }

    #[test]
    fn the_lens_is_taken_from_flags() {
        let cli =
            Cli::try_parse_from(["glimpse-ruler", "--lens-radius", "120", "--max-zoom", "12"])
                .unwrap();

        assert_eq!(cli.lens_radius, Some(120));
        assert_eq!(cli.max_zoom, Some(12));
    }

    #[test]
    fn there_is_no_output_format_to_choose() {
        for flag in ["--json", "--format"] {
            let error = Cli::try_parse_from(["glimpse-ruler", flag]).unwrap_err();

            assert_eq!(error.exit_code(), 2);
        }
    }
}
