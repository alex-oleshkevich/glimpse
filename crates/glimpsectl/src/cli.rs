use shadow_rs::shadow;

use std::ffi::OsString;
use std::path::PathBuf;

use chrono::NaiveTime;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
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

    #[arg(
        long = "panel",
        global = true,
        env = "GLIMPSE_PANEL_APP_ID",
        default_value = "me.aresa.GlimpsePanel",
        value_name = "APP_ID",
        help = "Panel D-Bus application ID"
    )]
    pub panel_app_id: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(subcommand, about = "The night light")]
    NightLight(NightLightCommand),

    #[command(subcommand, about = "Idle inhibitors and manual holds")]
    Idle(IdleCommand),

    #[command(subcommand, about = "Current conditions and the forecast")]
    Weather(WeatherCommand),

    #[command(subcommand, about = "Desktop light/dark mode")]
    Appearance(AppearanceCommand),

    #[command(subcommand, about = "Current location and solar phase")]
    Geolocation(GeolocationCommand),

    #[command(subcommand, about = "The notification store")]
    Notifications(NotificationsCommand),

    #[command(subcommand, about = "The panel's clipboard history")]
    Clipboard(ClipboardCommand),

    #[command(subcommand, about = "Inspect the configuration stack")]
    Config(ConfigCommand),

    #[command(subcommand, about = "Inspect, control and develop applets")]
    Applet(AppletCommand),

    #[command(subcommand, about = "Panel development tools")]
    Dev(DevCommand),

    #[command(about = "Generate shell completions")]
    Completions {
        #[arg(value_enum, help = "Shell to generate completions for")]
        shell: clap_complete::Shell,
    },

    #[command(about = "Show running glimpse components")]
    Status,

    #[command(about = "Inspect and control the panel")]
    Panel(PanelArgs),

    #[command(about = "Check the configuration and every provider")]
    Doctor,

    #[command(subcommand, about = "Restart, power off or suspend the system")]
    Session(SessionCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum SessionCommand {
    #[command(hide = true, about = "Declare glimpse:niri as the current desktop")]
    Declare,
    #[command(about = "Restart the system after confirmation")]
    Reboot,
    #[command(name = "power-off", about = "Power off the system after confirmation")]
    PowerOff,
    #[command(about = "Suspend the system immediately")]
    Suspend,
}

#[derive(Debug, Subcommand)]
pub enum NightLightCommand {
    #[command(about = "Print the current schedule and temperature")]
    Status {
        #[arg(long, help = "Follow status changes")]
        watch: bool,
    },
    #[command(about = "Hold neutral daylight until the current night ends")]
    Pause,
    #[command(about = "Follow the schedule again")]
    Resume,
    #[command(about = "Temporarily select a schedule")]
    Schedule {
        #[arg(help = "Schedule to run")]
        mode: Mode,
    },
    #[command(about = "Set or clear a temperature override")]
    Temperature {
        #[arg(value_parser = clap::value_parser!(u32).range(1..), required_unless_present = "clear", conflicts_with = "clear", help = "Temperature in Kelvin")]
        kelvin: Option<u32>,
        #[arg(long, help = "Clear the temperature override")]
        clear: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum AppearanceCommand {
    #[command(about = "Print desktop mode status")]
    Status {
        #[arg(long, help = "Follow status changes")]
        watch: bool,
    },
    #[command(about = "Temporarily select light or dark mode")]
    Set {
        #[arg(help = "Desktop mode")]
        mode: DesktopMode,
    },
    #[command(about = "Follow the configured mode policy")]
    Resume,
}

#[derive(Debug, Subcommand)]
pub enum GeolocationCommand {
    #[command(about = "Print location and solar phase")]
    Status,
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
    Status {
        #[arg(long, help = "Follow inhibitor changes")]
        watch: bool,
    },

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
        #[arg(last = true, value_name = "CMD", num_args = 1.., help = "Command to run while holding the inhibitor")]
        command: Vec<String>,
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

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DesktopMode {
    Light,
    Dark,
}

impl From<DesktopMode> for glimpse_dbus::settings::Mode {
    fn from(mode: DesktopMode) -> Self {
        match mode {
            DesktopMode::Light => Self::Light,
            DesktopMode::Dark => Self::Dark,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum NotificationsCommand {
    #[command(about = "Print the notifications the store holds")]
    List {
        #[arg(long, help = "Follow notification changes")]
        watch: bool,
    },

    #[command(about = "Send a notification")]
    Send(NotificationSendArgs),

    #[command(about = "Remove a stored notification")]
    Remove {
        #[arg(help = "Notification ID")]
        id: u32,
    },

    #[command(about = "Activate a notification's default action")]
    Activate {
        #[arg(help = "Notification ID")]
        id: u32,
    },

    #[command(about = "Invoke a notification action")]
    Action {
        #[arg(help = "Notification ID")]
        id: u32,
        #[arg(help = "Action key")]
        key: String,
    },

    #[command(about = "Mark a notification read and drop its popup")]
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
        about = "Turn do not disturb on or off, or toggle it",
        long_about = "Turn do not disturb on or off, or toggle it. Use `on --until HH:MM` to \
                      turn it off again at a local time."
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

#[derive(Debug, Args)]
pub struct NotificationSendArgs {
    #[arg(help = "Notification summary")]
    pub summary: String,
    #[arg(long, default_value = "", help = "Notification body")]
    pub body: String,
    #[arg(long, default_value = "glimpsectl", help = "Sending application name")]
    pub app_name: String,
    #[arg(long, default_value = "", help = "Icon name or path")]
    pub icon: String,
    #[arg(long, help = "Image path")]
    pub image: Option<String>,
    #[arg(long, default_value = "normal", help = "Notification urgency")]
    pub urgency: NotificationUrgency,
    #[arg(long, value_name = "KEY=LABEL", help = "Add an action")]
    pub action: Vec<String>,
    #[arg(long, default_value_t = 0, help = "Replace this notification ID")]
    pub replace: u32,
    #[arg(long, default_value_t = -1, allow_hyphen_values = true, help = "Expiry in milliseconds; -1 uses the server default")]
    pub expire: i32,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

impl NotificationUrgency {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::Critical => 2,
        }
    }
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
    Remove(EntryTarget),

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
    Toggle,
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
pub enum AppletCommand {
    #[command(about = "List live applets or the installed catalog")]
    List {
        #[arg(
            long,
            conflicts_with = "installed",
            help = "Follow live applet changes"
        )]
        watch: bool,
        #[arg(long, help = "List installed external applets")]
        installed: bool,
    },
    #[command(about = "Inspect a live applet or installed desktop entry")]
    Inspect {
        #[command(flatten)]
        target: AppletTarget,
        #[arg(long, conflicts_with_all = ["output", "edge"], help = "Inspect the installed desktop entry")]
        installed: bool,
    },
    #[command(about = "Click an applet")]
    Click {
        #[command(flatten)]
        target: AppletTarget,
        #[arg(long, default_value = "left", value_parser = button, help = "Mouse button: left, middle, right, or a number")]
        button: u32,
    },
    #[command(about = "Scroll an applet")]
    Scroll {
        #[command(flatten)]
        target: AppletTarget,
        #[arg(help = "Scroll direction")]
        direction: Direction,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..), help = "Number of scroll notches")]
        steps: u32,
    },
    #[command(about = "Open an applet's popover")]
    Open(AppletTarget),
    #[command(about = "Close an applet's popover")]
    Close(AppletTarget),
    #[command(about = "Toggle an applet's popover")]
    Toggle(AppletTarget),
    #[command(about = "Restart the applet's systemd services")]
    Restart {
        #[arg(help = "Applet desktop-file ID")]
        id: String,
    },
    #[command(about = "Create an external applet")]
    New(NewAppletArgs),
    #[command(about = "Run an applet in the panel until Ctrl-C")]
    Run(RunArgs),
    #[command(about = "Build an applet package (.glimpse.zip)")]
    Package(PackageArgs),
    #[command(about = "Install an external applet from a directory or a .glimpse.zip package")]
    Install {
        #[arg(
            default_value = ".",
            value_name = "DIR|PACKAGE",
            help = "Applet source directory or .glimpse.zip package"
        )]
        target: PathBuf,
    },
    #[command(about = "Remove an applet installed by glimpsectl")]
    Uninstall {
        #[arg(help = "Applet desktop-file ID")]
        id: String,
    },
}

#[derive(Debug, Args)]
pub struct AppletTarget {
    #[arg(value_name = "NAME", help = "Applet name, as list prints it")]
    pub name: String,
    #[arg(long, help = "Output connector")]
    pub output: Option<String>,
    #[arg(long, help = "Panel edge")]
    pub edge: Option<Position>,
}

#[derive(Debug, Subcommand)]
pub enum DevCommand {
    #[command(about = "Start an isolated panel in the foreground")]
    Panel(DevPanelArgs),
    #[command(about = "Close all popovers on the selected panel")]
    ClosePopovers,
}

#[derive(Debug, Args)]
pub struct DevPanelArgs {
    #[arg(long, help = "Fresh test-panel application ID")]
    pub app_id: Option<String>,
    #[arg(long, help = "Output connector")]
    pub output: Option<String>,
    #[arg(long, default_value = "bottom", help = "Panel edge")]
    pub edge: Position,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Applets to place on the test panel"
    )]
    pub applets: Vec<String>,
}

#[derive(Debug, Args)]
pub struct PackageArgs {
    #[arg(default_value = ".", help = "Applet source directory")]
    pub dir: PathBuf,
    #[arg(long, help = "Install the package after building it")]
    pub install: bool,
    #[arg(
        long,
        value_name = "PATH",
        help = "Write the package to this file or directory instead of ./<id>.glimpse.zip"
    )]
    pub out: Option<PathBuf>,
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
    #[command(subcommand)]
    pub command: PanelCommand,
}

#[derive(Debug, Subcommand)]
pub enum PanelCommand {
    #[command(about = "List running panel instances")]
    List {
        #[arg(long, help = "Follow running panel instances")]
        watch: bool,
    },
    #[command(about = "Reload the selected panel")]
    Reload,
    #[command(subcommand, hide = true)]
    Popovers(PopoversCommand),
}

#[derive(Debug, Subcommand)]
pub enum PopoversCommand {
    Close,
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
    #[arg(
        long = "bar",
        id = "bar",
        default_value_t = 0,
        help = "Configured panel index"
    )]
    pub panel: u32,
    #[arg(long, help = "Start and own a fresh test panel")]
    pub isolated: bool,
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
    #[arg(
        long,
        value_name = "FILE",
        help = "TOML with the applet's options and environment"
    )]
    pub applet_config: Option<PathBuf>,
}

pub fn completion_command() -> clap::Command {
    fn visible(command: &clap::Command) -> clap::Command {
        let mut result = clap::Command::new(command.get_name().to_owned())
            .args(
                command
                    .get_arguments()
                    .filter(|arg| !arg.is_hide_set())
                    .cloned(),
            )
            .subcommands(
                command
                    .get_subcommands()
                    .filter(|subcommand| !subcommand.is_hide_set())
                    .map(visible),
            );
        if let Some(about) = command.get_about() {
            result = result.about(about.clone());
        }
        if let Some(version) = command.get_version() {
            result = result.version(version.to_owned());
        }
        result
    }
    visible(&Cli::command())
}

pub fn parse() -> Cli {
    let arguments: Vec<_> = std::env::args_os().collect();
    Cli::try_parse_from(&arguments).unwrap_or_else(|error| {
        if error.exit_code() == 2
            && arguments
                .iter()
                .take_while(|argument| *argument != "--")
                .any(|argument| argument == "--json")
        {
            let _ = error.print();
            anstream::eprintln!(
                "{}",
                serde_json::json!({"error": error.to_string(), "exit": 2})
            );
            std::process::exit(2);
        }
        error.exit()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum AppletLanguage {
    Rs,
    Ts,
    Py,
    Go,
}

impl AppletLanguage {
    pub fn dir(self) -> &'static str {
        match self {
            Self::Rs => "rs",
            Self::Ts => "ts",
            Self::Py => "py",
            Self::Go => "go",
        }
    }
}

#[derive(Debug, Args)]
pub struct NewAppletArgs {
    #[arg(long, help = "Applet desktop-file ID")]
    pub id: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value = "ts",
        help = "Applet language template"
    )]
    pub language: AppletLanguage,
    #[arg(long, help = "Applet display name")]
    pub name: Option<String>,
    #[arg(long, help = "Applet description")]
    pub description: Option<String>,
    #[arg(long, help = "Applet icon name")]
    pub icon: Option<String>,
    #[arg(long, help = "Destination directory")]
    pub dir: Option<PathBuf>,
    #[arg(long, help = "Accept defaults without prompting")]
    pub yes: bool,
}

impl Command {
    pub fn needs_session_bus(&self) -> bool {
        !matches!(
            self,
            Self::Config(_)
                | Self::Doctor
                | Self::Completions { .. }
                | Self::Session(SessionCommand::Suspend)
                | Self::Dev(DevCommand::Panel(_))
                | Self::Applet(
                    AppletCommand::New(_)
                        | AppletCommand::Run(_)
                        | AppletCommand::Package(_)
                        | AppletCommand::Install { .. }
                        | AppletCommand::Uninstall { .. }
                        | AppletCommand::Restart { .. }
                        | AppletCommand::List {
                            installed: true,
                            ..
                        }
                        | AppletCommand::Inspect {
                            installed: true,
                            ..
                        }
                )
        )
    }
}

impl Cli {
    pub fn try_parse_from<I, T>(arguments: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let matches = Self::command().try_get_matches_from(arguments)?;
        let explicit_config =
            matches.value_source("config_dir") == Some(clap::parser::ValueSource::CommandLine);
        let mut cli = Self::from_arg_matches(&matches)?;
        if matches!(cli.command, Command::Dev(DevCommand::Panel(_))) && !explicit_config {
            cli.config.config_dir = None;
        }
        Ok(cli)
    }
}
