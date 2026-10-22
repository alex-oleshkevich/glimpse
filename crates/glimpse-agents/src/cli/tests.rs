use clap::{CommandFactory, Parser};

use super::{Cli, Command};

#[test]
fn the_argument_surface_is_well_formed() {
    Cli::command().debug_assert();
}

#[test]
fn no_subcommand_runs_the_agents_and_check_parses() {
    let cli = Cli::try_parse_from(["glimpse-agents"]).expect("parses");
    assert!(cli.command.is_none());
    let cli = Cli::try_parse_from(["glimpse-agents", "check"]).expect("parses");
    assert!(matches!(cli.command, Some(Command::Check)));
}
