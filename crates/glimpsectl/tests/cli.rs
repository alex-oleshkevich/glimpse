use clap::{CommandFactory, Parser};
use glimpsectl::cli::{
    AppletsCommand, Cli, Command, ConfigCommand, Mode, SunsetCommand, duration, temperature,
};

#[test]
fn the_argument_surface_is_well_formed() {
    Cli::command().debug_assert();
}

#[test]
fn a_missing_subcommand_is_a_usage_error() {
    let error = Cli::try_parse_from(["glimpsectl"]).expect_err("a subcommand is required");
    assert_eq!(error.exit_code(), 2);
}

#[test]
fn json_is_accepted_after_the_subcommand() {
    let cli = Cli::try_parse_from(["glimpsectl", "sunset", "status", "--json"])
        .expect("a global flag follows its subcommand");
    assert!(cli.json);
}

#[test]
fn every_mode_spells_itself_the_way_the_document_does() {
    assert_eq!(Mode::Off.as_str(), "off");
    assert_eq!(Mode::Automatic.as_str(), "automatic");
    assert_eq!(Mode::Schedule.as_str(), "schedule");
}

#[test]
fn config_doctor_and_applets_run_without_a_bus() {
    assert!(!Command::Doctor.needs_session_bus());
    assert!(!Command::Config(ConfigCommand::Path).needs_session_bus());
    assert!(!Command::Applets(AppletsCommand::List).needs_session_bus());
    assert!(Command::Sunset(SunsetCommand::Status).needs_session_bus());
}

#[test]
fn the_session_command_is_hidden_from_the_top_level_help() {
    let command = Cli::command();
    let session = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "session")
        .expect("the session subcommand still exists for parsing");
    assert!(session.is_hide_set());
}

#[test]
fn session_declare_still_parses() {
    let error = Cli::try_parse_from(["glimpsectl", "session", "declare", "--help"])
        .expect_err("--help exits through the error path");
    assert_eq!(error.exit_code(), 0);
}

fn run(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(["glimpsectl", "applets", "run"].iter().chain(args))
}

#[test]
fn running_an_applet_needs_no_session_bus() {
    let cli = run(&["me.x.Applet"]).unwrap();
    assert!(!cli.command.needs_session_bus());
}

#[test]
fn zero_scroll_steps_are_a_usage_error() {
    let error = Cli::try_parse_from([
        "glimpsectl",
        "panel",
        "scroll",
        "DP-1",
        "clock",
        "down",
        "--steps",
        "0",
    ])
    .expect_err("scrolling must have at least one step");
    assert_eq!(error.exit_code(), 2);
}

#[test]
fn a_temperature_is_kelvin_or_clear_and_never_zero() {
    assert_eq!(temperature("4000"), Ok(4000));
    assert_eq!(temperature("clear"), Ok(0));
    assert!(temperature("0").is_err());
    assert!(temperature("warm").is_err());
}

#[test]
fn a_duration_reads_bare_seconds_and_the_three_units() {
    assert_eq!(duration("90"), Ok(90));
    assert_eq!(duration("90s"), Ok(90));
    assert_eq!(duration("15m"), Ok(900));
    assert_eq!(duration("2h"), Ok(7200));
    assert!(duration("0").is_err());
    assert!(duration("5d").is_err());
    assert!(duration("m").is_err());
    assert!(duration("9999999h").is_err());
}
