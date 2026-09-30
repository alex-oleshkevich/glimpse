use shadow_rs::shadow;

use std::path::PathBuf;

use chrono::NaiveTime;
use clap::{Args, Parser, Subcommand, ValueEnum};
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

    #[command(subcommand, about = "The panel's clipboard history")]
    Clipboard(ClipboardCommand),

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

pub fn temperature(raw: &str) -> Result<u32, String> {
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

pub fn duration(raw: &str) -> Result<u32, String> {
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

#[derive(Debug, Subcommand)]
pub enum ClipboardCommand {
    #[command(about = "Print the history, newest first")]
    List,

    #[command(about = "Write one entry's bytes to stdout, exactly as they were copied")]
    Get(EntryTarget),

    #[command(about = "Put one entry back on the clipboard")]
    Copy(EntryTarget),

    #[command(about = "Keep one entry through eviction and clearing")]
    Pin(EntryTarget),

    #[command(about = "Let a pinned entry be evicted again")]
    Unpin(EntryTarget),

    #[command(about = "Remove one entry from the history")]
    Forget(EntryTarget),

    #[command(about = "Remove every entry that is not pinned")]
    Clear,
}

#[derive(Debug, Args)]
pub struct EntryTarget {
    #[arg(value_name = "ID", help = "The entry's id, as `list` prints it")]
    pub id: u64,
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
    #[command(about = "Run an applet in the panel until Ctrl-C")]
    Run(RunArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "lower")]
pub enum Zone {
    Left,
    Center,
    Right,
}

impl Zone {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
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
pub struct RunArgs {
    #[arg(
        default_value = ".",
        value_name = "DIR|ID",
        help = "An applet directory holding one .desktop file, or an installed applet's desktop-file id"
    )]
    pub target: String,
    #[arg(long, help = "Restart the applet when its files or config change")]
    pub dev: bool,
    #[arg(long, default_value_t = 0, help = "Configured panel index")]
    pub panel: u32,
    #[arg(
        long,
        default_value = "",
        help = "Output connector, or the first mapped output"
    )]
    pub output: String,
    #[arg(long, help = "Panel zone")]
    pub zone: Option<Zone>,
    #[arg(
        long = "env",
        value_name = "KEY=VALUE",
        help = "Set an applet environment variable"
    )]
    pub env: Vec<String>,
    #[arg(long, help = "Print protocol lines on stderr")]
    pub trace: bool,
    #[arg(long, help = "Run without Landlock confinement")]
    pub no_sandbox: bool,
    #[arg(
        long,
        value_name = "FILE",
        help = "TOML with the applet's options and sandbox grants; relative grants resolve against its directory"
    )]
    pub applet_config: Option<PathBuf>,
}

pub fn parse() -> Cli {
    Cli::parse()
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
