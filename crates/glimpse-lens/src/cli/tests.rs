use super::*;

#[test]
fn the_default_is_a_plain_magnifier() {
    Cli::command().debug_assert();
    let cli = Cli::try_parse_from(["glimpse-lens"]).unwrap();
    assert_eq!(cli.tool, None);
    assert!(cli.format.is_none());
    assert!(!cli.json);
    assert_eq!(cli.radius, 180);
    assert_eq!(cli.zoom, 3);
    assert!(cli.validate().is_ok());
}

#[test]
fn picker_accepts_its_output_flags() {
    let cli = Cli::try_parse_from([
        "glimpse-lens",
        "--tool",
        "picker",
        "--format",
        "oklch",
        "--json",
    ])
    .unwrap();
    assert_eq!(cli.tool, Some(Tool::Picker));
    assert_eq!(cli.format, Some(ColorFormat::Oklch));
    assert!(cli.json);
    assert!(cli.validate().is_ok());
}

#[test]
fn picker_output_flags_require_picker_mode() {
    for arguments in [
        vec!["glimpse-lens", "--json"],
        vec!["glimpse-lens", "--format", "hex"],
        vec!["glimpse-lens", "--tool", "ruler", "--json"],
        vec!["glimpse-lens", "--tool", "ruler", "--format", "hex"],
    ] {
        let cli = Cli::try_parse_from(arguments).unwrap();
        assert_eq!(cli.validate().unwrap_err().exit_code(), 2);
    }
}

#[test]
fn ruler_accepts_lens_overrides_and_unknown_tools_are_rejected() {
    let cli = Cli::try_parse_from([
        "glimpse-lens",
        "--tool",
        "ruler",
        "--radius",
        "120",
        "--zoom",
        "12",
    ])
    .unwrap();
    assert_eq!(cli.tool, Some(Tool::Ruler));
    assert_eq!(cli.radius, 120);
    assert_eq!(cli.zoom, 12);
    assert!(cli.validate().is_ok());
    assert!(Cli::try_parse_from(["glimpse-lens", "--tool", "unknown"]).is_err());
}

#[test]
fn configuration_paths_are_rejected() {
    let error = Cli::try_parse_from(["glimpse-lens", "--config", "/tmp/config.toml"]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    assert_eq!(error.exit_code(), 2);
}
