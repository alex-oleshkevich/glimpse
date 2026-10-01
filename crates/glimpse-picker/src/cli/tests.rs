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
