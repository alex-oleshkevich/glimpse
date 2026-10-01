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
        Cli::try_parse_from(["glimpse-ruler", "--lens-radius", "120", "--max-zoom", "12"]).unwrap();

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
