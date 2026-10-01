use super::*;

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(args).and_then(Cli::checked)
}

#[test]
fn the_argument_surface_is_well_formed() {
    Cli::command().debug_assert();
}

#[test]
fn no_subcommand_is_the_daemon_and_standalone_belongs_to_it() {
    let cli = parse(&["glimpse-lock", "--standalone"]).expect("parses");
    assert!(cli.command.is_none() && cli.standalone);
    assert!(parse(&["glimpse-lock", "--standalone", "lock"]).is_err());
    assert!(parse(&["glimpse-lock", "lock", "--standalone"]).is_err());
    let cli = parse(&["glimpse-lock", "check"]).expect("parses");
    assert!(matches!(cli.command, Some(Command::Check)));
}

#[test]
fn the_config_can_come_before_a_subcommand() {
    let cli = parse(&["glimpse-lock", "--config", "/tmp/x.toml", "check"]).expect("parses");
    assert!(matches!(cli.command, Some(Command::Check)));
    assert_eq!(
        cli.config.config.as_deref(),
        Some(std::path::Path::new("/tmp/x.toml"))
    );
}
