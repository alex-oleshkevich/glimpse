use shadow_rs::shadow;

use std::path::PathBuf;

use chrono::NaiveTime;
use clap::parser::ValueSource;
use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use serde::Serialize;

fn clock(raw: &str) -> Result<NaiveTime, String> {
    glimpse_config::parse_clock(raw)
}

shadow!(build);

#[derive(Debug, Parser)]
#[command(
    name = "glimpsectl",
    about = "Read and drive the glimpse services.",
    version = build::VERSION
)]
pub struct Cli {
    #[command(flatten)]
    pub config: glimpse_utils::ConfigArg,

    #[command(flatten)]
    pub log: glimpse_utils::LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,

    #[arg(
        long,
        global = true,
        help = "Print the payload as JSON instead of a table"
    )]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(subcommand, about = "The night light")]
    Sunset(SunsetCommand),

    #[command(subcommand, about = "Idle inhibitors and manual holds")]
    Idle(IdleCommand),

    #[command(subcommand, about = "Current conditions and the forecast")]
    Weather(WeatherCommand),

    #[command(subcommand, about = "The notification store")]
    Notifications(NotificationsCommand),

    #[command(subcommand, about = "Inspect the configuration stack")]
    Config(ConfigCommand),

    #[command(subcommand, about = "Inspect and control external applets")]
    Applets(AppletsCommand),

    #[command(about = "Show running glimpse components")]
    Status,

    #[command(about = "Inspect and control the panel")]
    Panel(PanelArgs),

    #[command(about = "Check the configuration and every provider")]
    Doctor,

    #[command(subcommand, hide = true)]
    Session(SessionCommand),
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    #[command(hide = true, about = "Declare glimpse:niri as the current desktop")]
    Declare,
}

#[derive(Debug, Subcommand)]
pub enum SunsetCommand {
    #[command(about = "Print the mode, the temperature applied and why it is not applying one")]
    Status,

    #[command(
        about = "Change the mode in force",
        long_about = "Change the mode in force. The document is not written: the mode lasts until \
                      `[night-light]` is edited or glimpse-sunset restarts."
    )]
    Mode {
        #[arg(value_name = "MODE", help = "Which schedule to run")]
        mode: Mode,
    },

    #[command(
        about = "Hold a fixed temperature, or clear it",
        long_about = "Hold a fixed temperature, or clear it. The document is not written: the \
                      temperature lasts until it is cleared or glimpse-sunset restarts."
    )]
    Temperature {
        #[arg(
            value_name = "KELVIN|clear",
            value_parser = temperature,
            help = "A color temperature in Kelvin, or `clear` to follow the schedule again"
        )]
        kelvin: u32,
    },
}

fn temperature(raw: &str) -> Result<u32, String> {
    match raw {
        "clear" => Ok(0),
        _ => raw
            .parse()
            .ok()
            .filter(|kelvin| *kelvin > 0)
            .ok_or_else(|| format!("expected a temperature in Kelvin or `clear`, got {raw:?}")),
    }
}

#[derive(Debug, Subcommand)]
pub enum IdleCommand {
    #[command(about = "Print every inhibitor and the health of each source")]
    Status,

    #[command(
        about = "Hold off idle and suspend while this command runs",
        long_about = "Hold off idle and suspend while this command runs. The hold's id is printed \
                      first; the hold ends on Ctrl-C, when `--for` lapses, or when the process \
                      exits for any reason."
    )]
    Inhibit {
        #[arg(
            long = "for",
            value_name = "DURATION",
            value_parser = duration,
            help = "End the hold after this long: 90, 90s, 15m or 2h"
        )]
        lapse: Option<u32>,
    },

    #[command(about = "Release a releasable inhibitor")]
    Release {
        #[arg(value_name = "ID", help = "The inhibitor's id, as `status` prints it")]
        id: u64,
    },
}

fn duration(raw: &str) -> Result<u32, String> {
    let (number, unit) = match raw.find(|character: char| !character.is_ascii_digit()) {
        Some(split) => raw.split_at(split),
        None => (raw, "s"),
    };
    let scale = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        _ => 0,
    };
    number
        .parse::<u32>()
        .ok()
        .and_then(|count| count.checked_mul(scale))
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| {
            format!("expected a positive duration such as 90, 90s, 15m or 2h, got {raw:?}")
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum Mode {
    Off,
    Automatic,
    Schedule,
}

impl From<Mode> for glimpse_config::Schedule {
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::Off => Self::Off,
            Mode::Automatic => Self::Automatic,
            Mode::Schedule => Self::Schedule,
        }
    }
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        glimpse_config::Schedule::from(self).as_str()
    }
}

#[derive(Debug, Subcommand)]
pub enum WeatherCommand {
    #[command(about = "Print every watched place and its current conditions")]
    Status,

    #[command(about = "Fetch now instead of waiting for the next poll")]
    Refresh,
}

#[derive(Debug, Subcommand)]
pub enum NotificationsCommand {
    #[command(about = "Print the notifications the store holds")]
    List,

    #[command(about = "Dismiss one notification")]
    Dismiss {
        #[arg(value_name = "ID", help = "The notification's id, as `list` prints it")]
        id: u32,
    },

    #[command(about = "Dismiss every notification, or every one from a single application")]
    Clear {
        #[arg(
            long,
            value_name = "APP_ID",
            help = "Only this application's notifications"
        )]
        app: Option<String>,
    },

    #[command(
        about = "Turn do not disturb on or off",
        long_about = "Turn do not disturb on or off. Without `--until` it stands until it is \
                      turned off again."
    )]
    Dnd {
        #[arg(value_name = "STATE", help = "Whether to silence notifications")]
        state: DndState,

        #[arg(
            long,
            value_name = "HH:MM",
            value_parser = clock,
            help = "Turn it off again at this local time, today or tomorrow"
        )]
        until: Option<NaiveTime>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum DndState {
    On,
    Off,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    #[command(about = "Print the merged configuration")]
    Show,

    #[command(about = "Validate a file, or the layered stack, and report where a problem is")]
    Validate {
        #[arg(value_name = "PATH", help = "Validate this file instead of the stack")]
        path: Option<PathBuf>,
    },

    #[command(about = "Print the files the layered stack resolved to, in order")]
    Path,
}

#[derive(Debug, Subcommand)]
pub enum AppletsCommand {
    #[command(about = "Create an external applet")]
    New(NewAppletArgs),
    #[command(about = "Validate an external applet")]
    Check {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    #[command(about = "Link an applet into the live panel until interrupted")]
    Dev {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    #[command(about = "Build a distributable applet")]
    Bundle {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long, default_value = "/usr")]
        prefix: PathBuf,
        #[arg(long, default_value = "dist")]
        out: PathBuf,
    },
    #[command(about = "Install an applet for the current user")]
    Install {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    #[command(about = "Remove a user-installed applet")]
    Uninstall { id: String },
    #[command(about = "List installed and placed external applets")]
    List,
    #[command(about = "Inspect an external applet")]
    Inspect {
        #[arg(value_name = "ID", help = "The applet's desktop-file id")]
        id: String,
    },
    #[command(about = "Stop running scopes so the panel restarts the applet")]
    Restart {
        #[arg(value_name = "ID", help = "The applet's desktop-file id")]
        id: String,
    },
    #[command(about = "Run an applet script under Deno")]
    Run(RunAppletArgs),
}

#[derive(Debug, Args)]
pub struct PanelArgs {
    #[arg(
        long,
        global = true,
        env = "GLIMPSE_PANEL_APP_ID",
        default_value = "me.aresa.GlimpsePanel",
        help = "Panel D-Bus application ID"
    )]
    pub app_id: String,
    #[command(subcommand)]
    pub command: PanelCommand,
}

#[derive(Debug, Subcommand)]
pub enum PanelCommand {
    #[command(about = "List the panel's applets")]
    Applets {
        #[arg(long, help = "Print applet changes until the panel exits")]
        watch: bool,
    },
    #[command(about = "Click an applet")]
    Click {
        #[arg(help = "Output connector")]
        connector: String,
        #[arg(help = "Applet name")]
        applet: String,
        #[arg(long, default_value = "left", value_parser = button, help = "Mouse button: left, middle, right, or a number")]
        button: u32,
        #[arg(long, help = "Panel edge")]
        position: Option<Position>,
    },
    #[command(about = "Scroll an applet")]
    Scroll {
        #[arg(help = "Output connector")]
        connector: String,
        #[arg(help = "Applet name")]
        applet: String,
        #[arg(help = "Scroll direction")]
        direction: Direction,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..), help = "Number of scroll notches")]
        steps: u32,
        #[arg(long, help = "Panel edge")]
        position: Option<Position>,
    },
    #[command(subcommand, about = "Control one applet's popover")]
    Popover(PopoverCommand),
    #[command(subcommand, about = "Control all panel popovers")]
    Popovers(PopoversCommand),
}

#[derive(Debug, Subcommand)]
pub enum PopoverCommand {
    #[command(about = "Open the applet's popover")]
    Open(AppletTarget),
    #[command(about = "Close the applet's popover")]
    Close(AppletTarget),
    #[command(about = "Open it when closed, close it when open")]
    Toggle(AppletTarget),
}

#[derive(Debug, Subcommand)]
pub enum PopoversCommand {
    Close,
}

#[derive(Debug, Args)]
pub struct AppletTarget {
    #[arg(help = "Output connector")]
    pub connector: String,
    #[arg(help = "Applet name")]
    pub applet: String,
    #[arg(long, help = "Panel edge")]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize)]
#[value(rename_all = "lower")]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Top,
    Bottom,
    Left,
    Right,
}

impl std::str::FromStr for Position {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        })
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "lower")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    pub fn delta(self) -> (f64, f64) {
        match self {
            Self::Up => (0.0, -1.0),
            Self::Down => (0.0, 1.0),
            Self::Left => (-1.0, 0.0),
            Self::Right => (1.0, 0.0),
        }
    }
}

fn button(value: &str) -> Result<u32, String> {
    match value {
        "left" => Ok(1),
        "middle" => Ok(2),
        "right" => Ok(3),
        _ => value
            .parse()
            .map_err(|_| format!("expected left, middle, right, or a number, got {value}")),
    }
}

#[derive(Debug, Args)]
pub struct RunAppletArgs {
    #[arg(long, value_parser = grant_list, help = "Hosts the applet may reach")]
    pub allow_net: Option<String>,
    #[arg(long, value_parser = grant_list, help = "Paths the applet may read")]
    pub allow_read: Option<String>,
    #[arg(long, value_parser = grant_list, help = "Environment variables the applet may read")]
    pub allow_env: Option<String>,
    #[arg(long, help = "Restart the applet when its files change")]
    pub watch: bool,
    #[arg(help = "The applet script (.js, .ts or .tsx)")]
    pub entry: PathBuf,
}

pub fn parse() -> Cli {
    let matches = Cli::command().get_matches();
    if let Some(flag) = stray_run_flag(&matches) {
        Cli::command()
            .error(
                clap::error::ErrorKind::ArgumentConflict,
                format!("`applets run` does not take --{flag}"),
            )
            .exit();
    }
    Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit())
}

fn stray_run_flag(matches: &ArgMatches) -> Option<String> {
    let Some(("applets", applets)) = matches.subcommand() else {
        return None;
    };
    let Some(("run", run)) = applets.subcommand() else {
        return None;
    };
    Cli::command()
        .get_arguments()
        .filter(|arg| arg.is_global_set())
        .map(|arg| arg.get_id().as_str())
        .find(|id| run.value_source(id) == Some(ValueSource::CommandLine))
        .map(|id| id.replace('_', "-"))
}

fn grant_list(value: &str) -> Result<String, String> {
    if value.split(',').any(|item| item.trim().is_empty()) {
        Err("grant list must contain nonempty values".into())
    } else {
        Ok(value.into())
    }
}

#[derive(Debug, Args)]
pub struct NewAppletArgs {
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub icon: Option<String>,
    #[arg(long, conflicts_with = "no_popover")]
    pub popover: bool,
    #[arg(long, conflicts_with = "popover")]
    pub no_popover: bool,
    #[arg(long)]
    pub allow_net: Option<String>,
    #[arg(long)]
    pub dir: Option<PathBuf>,
    #[arg(long)]
    pub yes: bool,
}

impl Command {
    pub fn needs_session_bus(&self) -> bool {
        !matches!(self, Self::Config(_) | Self::Doctor | Self::Applets(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn json_is_accepted_after_the_subcommand() {
        let cli = Cli::try_parse_from(["glimpsectl", "sunset", "status", "--json"])
            .expect("a global flag follows its subcommand");
        assert!(cli.json);
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
    fn dangerous_grants_are_unknown_arguments() {
        for grant in [
            "--allow-run",
            "-A",
            "--allow-write",
            "--allow-ffi",
            "--allow-sys",
        ] {
            assert!(run(&[grant, "main.ts"]).is_err());
        }
    }

    #[test]
    fn empty_grants_are_rejected() {
        for grant in [
            "--allow-net=",
            "--allow-net=a,",
            "--allow-read=,a",
            "--allow-read=a,,b",
            "--allow-env=",
            "--allow-net=a, ",
        ] {
            assert!(run(&[grant, "main.ts"]).is_err());
        }
    }

    #[test]
    fn an_entry_that_looks_like_a_flag_stays_a_path() {
        let cli = run(&["--", "--allow-run=x.ts"]).unwrap();
        let Command::Applets(AppletsCommand::Run(args)) = cli.command else {
            panic!("not the run command");
        };
        assert_eq!(args.entry, PathBuf::from("--allow-run=x.ts"));
    }

    #[test]
    fn applets_run_refuses_global_flags() {
        let stray =
            |args: &[&str]| stray_run_flag(&Cli::command().try_get_matches_from(args).unwrap());
        assert_eq!(stray(&["glimpsectl", "applets", "run", "main.ts"]), None);
        assert_eq!(
            stray(&["glimpsectl", "applets", "run", "--json", "main.ts"]).as_deref(),
            Some("json")
        );
        assert_eq!(
            stray(&["glimpsectl", "--json", "applets", "run", "main.ts"]).as_deref(),
            Some("json")
        );
        assert_eq!(stray(&["glimpsectl", "--json", "applets", "list"]), None);
    }

    #[test]
    fn running_an_applet_needs_no_session_bus() {
        let cli = run(&["main.ts"]).unwrap();
        assert!(!cli.command.needs_session_bus());
    }
}
