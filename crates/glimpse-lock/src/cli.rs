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
mod tests;
