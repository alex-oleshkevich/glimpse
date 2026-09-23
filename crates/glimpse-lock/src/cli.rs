use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, Subcommand};
use glimpse_utils::{ConfigArg, LogArgs};
use shadow_rs::shadow;

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-lock",
    about = "Lock screen.",
    version = build::VERSION
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(
        long,
        help = "Development mode: lock at once without logind, exit after a PAM-authenticated unlock"
    )]
    pub standalone: bool,

    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}

impl Cli {
    pub fn checked(self) -> Result<Self, clap::Error> {
        if self.standalone && self.command.is_some() {
            return Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "--standalone runs the daemon and cannot be combined with a subcommand",
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Ask logind to lock this session and wait until it is locked")]
    Lock,
    #[command(about = "Diagnose why a correct password would be rejected")]
    Check,
}

#[cfg(test)]
mod tests {
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
}
