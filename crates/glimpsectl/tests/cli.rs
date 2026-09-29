use std::path::PathBuf;

use clap::{CommandFactory, Parser};
use glimpsectl::cli::{
    AppletsCommand, Cli, Command, ConfigCommand, Mode, SunsetCommand, stray_run_flag,
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
fn run_accepts_executable_and_trailing_arguments() {
    let cli = run(&["/tmp/applet.mjs", "--verbose", "hello"]).unwrap();
    let Command::Applets(AppletsCommand::Run(args)) = cli.command else {
        panic!("not the run command");
    };
    assert_eq!(args.exe, PathBuf::from("/tmp/applet.mjs"));
    assert_eq!(args.args, ["--verbose", "hello"]);
    let cli = run(&["--allow-net=x", "foo.js"]).unwrap();
    let Command::Applets(AppletsCommand::Run(args)) = cli.command else {
        panic!("not the run command");
    };
    assert_eq!(args.exe, PathBuf::from("--allow-net=x"));
    assert_eq!(args.args, ["foo.js"]);
}

#[test]
fn applets_run_refuses_global_flags() {
    let stray = |args: &[&str]| stray_run_flag(&Cli::command().try_get_matches_from(args).unwrap());
    assert_eq!(stray(&["glimpsectl", "applets", "run", "applet.mjs"]), None);
    assert_eq!(
        stray(&["glimpsectl", "applets", "run", "--json", "applet.mjs"]).as_deref(),
        Some("json")
    );
    assert_eq!(
        stray(&["glimpsectl", "--json", "applets", "run", "applet.mjs"]).as_deref(),
        Some("json")
    );
    assert_eq!(stray(&["glimpsectl", "--json", "applets", "list"]), None);
}

#[test]
fn running_an_applet_needs_no_session_bus() {
    let cli = run(&["applet.mjs"]).unwrap();
    assert!(!cli.command.needs_session_bus());
}
