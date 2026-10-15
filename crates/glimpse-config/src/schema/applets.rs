use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// One applet on a bar and the settings its kind understands.
/// The table name is the applet's name, and `extends` says which kind it is when the two differ —
/// which is how one kind can appear more than once, as `[applets.clock-utc]` with
/// `extends = "clock"`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Applet {
    #[serde(flatten)]
    pub kind: Kind,
    /// The document's `[regional]` table, stamped on by whoever builds a running applet — for the
    /// panel that is `applets::resolve`, the one place that produces an `Applet` both by finding
    /// its table and by inventing one for a name that has none. An applet renders a time of day
    /// off the configuration it is already handed rather than through a second route. It is not a
    /// key of an applet's own table and never round-trips.
    #[serde(skip)]
    pub regional: super::Regional,
}

/// Which kind of applet this is, and the settings that kind alone understands.
pub const NETWORK_TOOLTIP: &str = "{connection}\n{metered}\nSignal {signal}%\nVPN: {vpn}";
pub const PRINTING_TOOLTIP: &str = "{document}\n{printer}\n{pages}";
pub const WEATHER_TOOLTIP: &str = "{place}\n{condition}, {temperature}, feels like {feels-like}";

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(
    tag = "extends",
    rename_all = "kebab-case",
    rename_all_fields = "kebab-case",
    deny_unknown_fields
)]
pub enum Kind {
    /// Output volume, with the default sink and per-application streams in its popover.
    Audio(Audio),
    /// Battery, power mode, keep awake and peripheral batteries in its popover.
    Power(Power),
    /// Adapter state and paired devices.
    Bluetooth(Bluetooth),
    /// Clipboard history.
    Clipboard(Clipboard),
    /// The time and date, with a calendar in its popover.
    Clock(Clock),
    /// Picks a color from the screen on right click; the popover keeps the picked colors.
    ColorPicker {
        /// What the bar's tooltip reads. `{color}` is the last picked color, in the notation chosen
        /// in the popover.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schemars(extend("default" = "{color}"))]
        tooltip_format: Option<String>,
    },
    /// A chip the user defines, running a program on a click or a scroll.
    Command(Box<Command>),
    /// Connected outputs, brightness and night light in one popover.
    Display(Display),
    /// A third-party applet found through its `.desktop` file (`Implements=me.aresa.Glimpse.Applet1`).
    Exec(Box<Exec>),
    /// A counter that ticks once a second. A development fixture: it proves the service is
    /// reachable and events are arriving, and is not meant for a real bar.
    Heartbeat {},
    /// Idle inhibition, for keeping the screen awake.
    Idle {
        /// What the bar's tooltip reads. `{status}` says what is keeping the screen awake, if
        /// anything.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schemars(extend("default" = "{status}"))]
        tooltip_format: Option<String>,
    },
    /// A phone paired through KDE Connect: its battery on the bar, and ring, ping, send and pair
    /// in its popover. Needs `kdeconnectd` running; it is never started from here.
    Kdeconnect(Kdeconnect),
    /// The active keyboard layout, and switches between the configured ones.
    Keyboard {
        /// What the bar's tooltip reads. `{code}` is the layout's short label and `{name}` its full
        /// name.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schemars(extend("default" = "{name}"))]
        tooltip_format: Option<String>,
    },
    /// The currently playing track, with transport controls in its popover.
    Mpris(Mpris),
    /// Connection state, with the available networks in its popover.
    Network(Network),
    /// The next entry from the configured calendars.
    NextEvent(NextEvent),
    /// Unread notifications, with their history in its popover.
    Notifications(Notifications),
    /// A strip of workspaces or windows, one slot each, that switches between them on a click.
    Pager(Pager),
    /// Home and user directories, bookmarks, network shares and the trash.
    Places(Places),
    /// Active print jobs.
    Printing(Printing),
    /// Shows when the microphone, camera or screen is in use.
    Privacy(Privacy),
    /// Removable drives, one row each, with open and eject in its popover.
    Removable(Removable),
    /// Measures a distance on screen on right click; the popover keeps recent measurements.
    Ruler {
        /// What the bar's tooltip reads. `{measurement}` is the last distance measured.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schemars(extend("default" = "{measurement}"))]
        tooltip_format: Option<String>,
    },
    /// Log out, suspend, restart and shut down.
    Session {
        /// What the bar's tooltip reads. `{user}` is your user name.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schemars(extend("default" = "Session"))]
        tooltip_format: Option<String>,
    },
    /// Live CPU, RAM, swap, disk, network and (amdgpu) GPU load. Nothing is sampled unless the
    /// applet is on a panel.
    SystemMonitor(SystemMonitor),
    /// The system tray: icons from applications that ask for one.
    Tray(Tray),
    /// Current conditions, with the forecast in its popover.
    Weather(Weather),
    /// A popover with a colored block in every slot of the shell. A development fixture, mirrored by
    /// `sdk/examples/widget_demo`, for comparing built-in and exec rendering; not meant for a real bar.
    WidgetDemo {},
    /// The current workspace's name, renamed from its popover.
    WorkspaceName {
        /// What the bar's tooltip reads. `{index}`, `{id}`, `{name}`, `{name-or-index}` and
        /// `{workspace-name}`, the same tokens as the pager's `label`. Unset shows the workspace
        /// and its monitor.
        #[serde(skip_serializing_if = "Option::is_none")]
        tooltip_format: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Exec {
    /// The desktop-file id, e.g. `me.aresa.GlimpseTodo` for the todo example.
    pub applet: String,
    /// Passed to the applet as written.
    #[serde(default)]
    pub options: serde_json::Map<String, serde_json::Value>,
    /// Variables set for the applet process on top of the panel's environment.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl Exec {
    pub fn validate(&self) -> Result<(), String> {
        validate_env(&self.env)
    }
}

const EXEC_ENV_DENIED: [&str; 5] = [
    "INVOCATION_ID",
    "JOURNAL_STREAM",
    "NOTIFY_SOCKET",
    "MANAGERPID",
    "SYSTEMD_EXEC_PID",
];

pub fn exec_env_name_allowed(name: &str) -> bool {
    !EXEC_ENV_DENIED.contains(&name)
        && !name.starts_with("LISTEN_")
        && !name.starts_with("WATCHDOG_")
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn validate_env(env: &BTreeMap<String, String>) -> Result<(), String> {
    for (name, value) in env {
        if !exec_env_name_allowed(name) {
            return Err(format!("env name is invalid: {name:?}"));
        }
        if value.contains('\0') {
            return Err(format!("env value for {name:?} contains NUL"));
        }
    }
    Ok(())
}

/// Settings for the tray applet. Which items exist is the applications' decision; this is only
/// which of them the bar shows and how many fit before the rest go behind the chevron.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Tray {
    /// Item ids never shown, matched against the application's own `Id`. That id survives an
    /// application restart, which the bus name it happens to hold does not.
    pub hide: Vec<String>,
    /// Item ids kept on the bar whatever the cap, in the order given. Anything not named here
    /// follows in the order the items registered.
    pub pin: Vec<String>,
    /// How many icons stay on the bar; the rest open from the chevron beside them. `0` keeps every
    /// icon on the bar and shows no chevron.
    pub max_visible: u8,
}

/// Settings for the system-monitor applet's chips and popover coloring. What the sampler itself
/// reads lives in the top-level `[system-monitor]` table instead.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct SystemMonitor {
    /// What the bar's tooltip reads. Plain text, the same on every chip. Unset shows no tooltip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// Chips shown on the bar, in order. Anything sampled but left out here still appears in the
    /// popover — this list is panel real estate, not visibility. A repeated chip collapses to its
    /// first occurrence.
    #[serde(deserialize_with = "chips")]
    pub chips: Vec<Chip>,
    /// How each chip's own label reads. `{name}` is the chip's own localized name ("CPU", "RAM",
    /// …); `{value}` is its reading ("42%" for a percentage chip, "↓1.2 MB/s" for network, "62°C" for a
    /// temperature). An
    /// unrecognized token is left as literal text rather than silently emptied.
    pub chip_format: String,
    /// Percent at and above which a percentage reading (CPU, RAM, swap, GPU usage and memory) is
    /// shown as a warning. Network and temperatures have no percentage and are never colored.
    #[schemars(range(min = 1, max = 100))]
    pub warn_percent: u8,
    /// Percent at and above which a percentage reading is shown as critical. Must not be below
    /// `warn-percent`.
    #[schemars(range(min = 1, max = 100))]
    pub critical_percent: u8,
}

/// One chip the system-monitor applet can show on the bar. Disk is deliberately not a variant —
/// arbitrarily many configured paths don't reduce to one chip value, so disk stays popover-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Chip {
    Cpu,
    Ram,
    Swap,
    Network,
    Gpu,
    CpuTemp,
    GpuTemp,
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            chips: vec![Chip::Cpu, Chip::Ram],
            chip_format: "{name} {value}".to_owned(),
            warn_percent: 85,
            critical_percent: 95,
        }
    }
}

/// A chip the user defines: an icon, a label or both, and a program for each click and scroll
/// notch. Several sit side by side as `[applets.<name>]` tables with `extends = "command"`. Every
/// program is a command and its arguments, `["grim", "-g", "0,0 640x480"]`, with no shell between
/// here and the program. A gesture with no program does nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Command {
    /// What the bar's tooltip reads. Plain text. Unset shows no tooltip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// A themed icon name, or the absolute path to an image. Prefer a `-symbolic` name, which
    /// follows the bar's color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Text beside the icon. With neither an icon nor a label the chip takes no room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// What a left click runs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_click: Vec<String>,
    /// What a middle click runs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_middle_click: Vec<String>,
    /// What a right click runs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_right_click: Vec<String>,
    /// What each notch of scrolling up runs, once per notch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_scroll_up: Vec<String>,
    /// What each notch of scrolling down runs, once per notch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_scroll_down: Vec<String>,
    /// What each notch of scrolling left runs, once per notch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_scroll_left: Vec<String>,
    /// What each notch of scrolling right runs, once per notch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_scroll_right: Vec<String>,
}

/// The clipboard applet's own settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Clipboard {
    /// What the bar's tooltip reads. `{count}` is the number of entries held. Unset shows no
    /// tooltip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// What the chip reads beside its icon. `{count}` is the number of entries held. Left unset the
    /// chip is an icon alone, which is what every applet does unless asked otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_format: Option<String>,
    /// How many recent entries the popover lists. Nothing in the panel scrolls, so this is what
    /// keeps the card on the screen. Pinned entries are listed above and are not counted here —
    /// a pin is an explicit choice and is never dropped from the list. Clamped to 1..=50.
    pub visible: usize,
    /// How much of an entry a row shows before it is ellipsized, in characters. A row holds 128 at
    /// most and shows roughly 24 before ellipsizing, so this trades tooltip detail for nothing much
    /// on screen. Clamped to 8..=128.
    pub preview_chars: usize,
    /// Whether the chip is shown when nothing has been copied yet. Off, the applet takes no room
    /// until there is something to open it for.
    pub show_when_empty: bool,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            label_format: None,
            visible: 10,
            preview_chars: 72,
            show_when_empty: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Notifications {
    /// What the bar's tooltip reads. `{count}` is the number of unread notifications. Unset says
    /// how many are waiting and whether do not disturb is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// How the bar marks unread notifications: `icon-dot` adds a dot, `icon-counter` shows how many,
    /// `icon-only` shows the bell alone.
    pub indicator_style: NotificationIndicatorStyle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationIndicatorStyle {
    IconOnly,
    #[default]
    IconDot,
    IconCounter,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Power {
    /// What the bar's tooltip reads. `{percentage}`, `{state}` and `{remaining}`, the time left
    /// until empty or full.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "{state} · {percentage}"))]
    pub tooltip_format: Option<String>,
    /// Whether the bar shows the battery icon, the label, or both.
    pub indicator_style: BatteryIndicatorStyle,
    /// What the label shows. Placeholders are replaced by name: `{percentage}`, `{state}` and
    /// `{remaining}`, the time left to empty or full.
    pub label_format: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BatteryIndicatorStyle {
    IconOnly,
    #[default]
    IconText,
    Text,
}

impl Default for Power {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            indicator_style: BatteryIndicatorStyle::IconOnly,
            label_format: "{percentage}".to_owned(),
        }
    }
}

/// Settings for the kdeconnect applet. Which devices exist and which are paired is the daemon's
/// decision; this is only how the bar renders the one it follows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Kdeconnect {
    /// What the bar's tooltip reads. `{name}`, `{battery}` and `{charging}`, the same tokens as
    /// `label-format`. Unset lists every paired device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// Whether the bar shows the device icon, the label, or both.
    pub indicator_style: BatteryIndicatorStyle,
    /// What the label shows. Placeholders are replaced by name: `{name}`, `{battery}` and
    /// `{charging}`. A placeholder with nothing behind it renders as nothing, so a device with no
    /// battery never shows a bare `%`.
    pub label_format: String,
    /// The device the bar follows, by the name the device announces. Unset follows the first
    /// connected paired device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Hide the chip while no paired device is connected, instead of showing it with an offline
    /// emblem.
    pub hide_when_disconnected: bool,
}

impl Default for Kdeconnect {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            indicator_style: BatteryIndicatorStyle::IconText,
            label_format: "{battery}".to_owned(),
            device: None,
            hide_when_disconnected: false,
        }
    }
}

/// Settings for the mpris applet. Which players exist and which one is current is the service's
/// decision, in `[mpris]`; this is only how the bar renders the one it is given.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Mpris {
    /// What the bar's tooltip reads. The same tokens as `label-format`. Unset shows no tooltip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// What the bar shows. Placeholders are replaced by name: `{player}`, `{title}`, `{artist}`,
    /// `{album}`, `{state}`, `{position}`, `{duration}` and `{remaining}`. A placeholder with
    /// nothing behind it renders as nothing.
    pub label_format: String,
    /// Longest label the bar shows, in characters; the rest is ellipsized. Track titles come from
    /// whatever is playing and are unbounded, and a bar that resizes as they change is worse than
    /// one that truncates.
    pub max_length: u8,
}

/// The network applet's own settings. Where the networks are scanned and listed is `[network]`;
/// this is only what the chip on the bar does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Network {
    /// What the bar's tooltip reads. `{connection}` is the network you are on, `{metered}` reads
    /// Metered connection on a metered one, `{signal}` is the Wi-Fi signal in percent and `{vpn}`
    /// the active VPN's name. A line whose tokens are all empty is left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = NETWORK_TOOLTIP))]
    pub tooltip_format: Option<String>,
    /// How many networks the popover lists before the rest go behind a "more networks" row. `0`
    /// lists every network in range.
    pub visible_networks: usize,
    /// Whether an active VPN gets a chip of its own beside the network chip. An indicator is an
    /// icon, and two states overlaid on one glyph are illegible at bar size.
    pub vpn_chip: bool,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            visible_networks: 8,
            vpn_chip: true,
        }
    }
}

impl Default for Mpris {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            label_format: "{title} — {artist}".to_owned(),
            max_length: 40,
        }
    }
}

/// Settings for the clock applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Clock {
    /// What the bar's tooltip reads. A `strftime` format, such as `%A, %-d %B %Y`. Unset shows no
    /// tooltip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// A `strftime` format string for the bar, such as `%H:%M` or `%a %d %b %H:%M`.
    #[serde(alias = "format")]
    pub label_format: String,
    /// The IANA zone this clock reads, such as `UTC` or `Asia/Tokyo`. Unset is the local zone;
    /// naming one is how a second `[applets.clock-utc]` shows somewhere else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Whether the popover names the ISO week the shown month belongs to.
    pub week_numbers: bool,
    /// The other zones the popover lists under its world clock. Empty hides the section.
    pub timezones: Vec<Timezone>,
    /// Whether an all-day entry is left out of the calendar popover's day list entirely, rather
    /// than shown alongside the day's timed events.
    pub hide_all_day: bool,
}

/// The bluetooth applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Bluetooth {
    /// What the bar's tooltip reads. `{status}` says whether Bluetooth is on and how many devices
    /// are connected, `{devices}` lists the connected devices and `{adapter}` is the adapter's
    /// name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "{status}"))]
    pub tooltip_format: Option<String>,
    /// How many paired devices the popover lists before the rest go behind a drawer.
    pub devices: usize,
    /// How many nearby devices the popover lists while scanning.
    pub nearby: usize,
}

impl Default for Bluetooth {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            devices: 6,
            nearby: 8,
        }
    }
}

/// The places applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Places {
    /// What the bar's tooltip reads. `{bookmarks}` and `{places}` are how many of each the popover
    /// lists.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "Places"))]
    pub tooltip_format: Option<String>,
    /// How many bookmarks the popover lists before the rest go behind a drawer.
    pub bookmarks: usize,
}

impl Default for Places {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            bookmarks: 8,
        }
    }
}

/// The removable applet. How often free space is sampled is `[removable] capacity-interval`; this
/// is only how the bar and its popover behave.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Removable {
    /// What the bar's tooltip reads. `{drives}` and `{volumes}` are how many of each are attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "Removable"))]
    pub tooltip_format: Option<String>,
    /// How many devices (drives, not their volumes) the popover lists before the rest go behind a
    /// drawer.
    pub volumes: usize,
}

impl Default for Removable {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            volumes: 6,
        }
    }
}

/// The printing applet's own settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Printing {
    /// What the bar's tooltip reads. `{document}` is the document printing now, `{printer}` the
    /// printer, `{pages}` how far it has got and `{count}` how many jobs are waiting. Shown only
    /// while something is printing. A line whose tokens are all empty is left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = PRINTING_TOOLTIP))]
    pub tooltip_format: Option<String>,
    /// Print jobs listed in the popover before older ones are hidden.
    pub jobs: usize,
    /// When the bar shows the printing chip.
    pub show: PrintingShow,
}

impl Default for Printing {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            jobs: 8,
            show: PrintingShow::default(),
        }
    }
}

/// When the printing chip is on the bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PrintingShow {
    /// The chip stays on the bar whenever a printer exists, idle or not.
    Always,
    /// The chip appears only while a print job is active or a printer needs attention.
    #[default]
    Jobs,
}

/// The privacy applet's own settings: which of its sources it shows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Privacy {
    /// What the bar's tooltip reads. Plain text. Unset names what is in use and by which app.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// Whether the camera is shown when in use.
    pub show_camera: bool,
    /// Whether the microphone is shown when in use.
    pub show_microphone: bool,
    /// Whether screen capture is shown when in use.
    pub show_screencast: bool,
    /// Whether location access is shown when in use.
    pub show_location: bool,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            show_camera: true,
            show_microphone: true,
            show_screencast: true,
            show_location: true,
        }
    }
}

/// Settings for the audio applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Audio {
    /// What the bar's tooltip reads. `{volume}` is the volume in percent, or Muted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "{volume}"))]
    pub tooltip_format: Option<String>,
    /// How much one scroll notch on the speaker icon changes the volume, in percent, from 1 to 100.
    #[serde(deserialize_with = "scroll_percent")]
    #[schemars(range(min = 1, max = 100))]
    pub scroll_step: u8,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            scroll_step: 5,
        }
    }
}

/// Settings for the display applet. The floor a display can reach is `[brightness] minimum`;
/// this is only how the bar and its popover behave.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Display {
    /// What the bar's tooltip reads. `{name}` is the display's name and `{brightness}` its
    /// brightness in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "{name}: {brightness}%"))]
    pub tooltip_format: Option<String>,
    /// How much a scroll notch on the chip moves the current display, in percent of its range.
    /// Converted to native units at the call site and rounded away from zero, so a notch always
    /// moves something even on a display with a small native range. Zero would leave the wheel
    /// silently dead; the range is 1 to 100.
    #[serde(deserialize_with = "scroll_percent")]
    #[schemars(range(min = 1, max = 100))]
    pub scroll_step: u8,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            scroll_step: 5,
        }
    }
}

fn scroll_percent<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u8::deserialize(deserializer)?;
    (1..=100)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 1 and 100 percent"))
}

/// One zone in the clock popover's world clock.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Timezone {
    /// What the row is called, such as `Tokyo`.
    pub label: String,
    /// The IANA zone the row reads, such as `Asia/Tokyo`.
    pub timezone: String,
    /// A note after the `tomorrow` or `yesterday` the row works out for itself, such as
    /// `the office`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// An icon name replacing the sun or moon the row picks from the hour it is showing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Settings for the next-event applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct NextEvent {
    /// What the bar's tooltip reads. `{summary}` is the event's title, `{detail}` its location or
    /// description, `{when}` when it starts and `{conflicts}` the events overlapping it. Unset
    /// shows no tooltip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// How long before an event starts it takes the bar, in minutes. Anything further out than
    /// this leaves the applet empty, which is what keeps a meeting two days away off a bar that
    /// has nothing to say about it. An event that has already started stays until it ends,
    /// however long it has been running, so `0` shows only what is under way.
    pub within: u64,
    /// How close an event has to be before the bar spells out how long is left, in minutes. Inside
    /// it the indicator reads `Design review in 12 min`, or `ends in 25 min` once it has started;
    /// outside it the title stands alone. Set it below `within` to have an entry appear quietly and
    /// start counting only as it approaches; `0` never counts.
    pub countdown: u64,
    /// How far ahead the popover's list reaches, in minutes. This is the second of two windows and
    /// the wider one: `within` decides when the bar lights up, `horizon` decides what the list
    /// holds once you open it. The default 720 is twelve hours, so a late-afternoon glance still
    /// reaches tomorrow morning — a plain "today only" rule empties the list at exactly the hour
    /// tomorrow's first meeting starts mattering. 1440 is a rolling day.
    pub horizon: u64,
    /// Whether an all-day entry may take the bar. A timed event always wins over one, so this
    /// only decides what happens on a day holding nothing else — leaving it off keeps a week of
    /// leave from pinning the applet open for the whole week.
    pub all_day: bool,
    /// How many entries the popover lists under the one it is showing, whichever `horizon` leaves.
    /// Capped at 20, because a list longer than that is a popover taller than the screen.
    pub upcoming: usize,
}

/// Settings for the weather applet.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Weather {
    /// What the bar's tooltip reads. `{place}`, `{temperature}`, `{feels-like}` and `{condition}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = WEATHER_TOOLTIP))]
    pub tooltip_format: Option<String>,
    /// Which place this applet shows.
    pub place: Place,
    /// What the bar and the popover call this place. Unset means a fixed place reads as the pair
    /// it was given, and `here` as whatever the geolocation service resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// How many hours the popover's strip shows. The strip starts at the next hour — the hour
    /// standing is already the hero.
    pub hours: u8,
    /// How many days after today the popover's list shows. Today always leads the list, holding
    /// the current conditions.
    pub days: u8,
}

/// Where a weather applet looks. Tagged on `at`, matching the shape `WatchPlace` takes, so a
/// place reads the same in a document and on the wire.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "at", rename_all = "kebab-case", deny_unknown_fields)]
#[schemars(transform = coordinate_alias)]
pub enum Place {
    /// Follow the geolocation service, and move with it.
    Here {},
    /// A fixed pair of coordinates in degrees.
    #[serde(rename = "latlon", alias = "coordinates")]
    Coordinates {
        /// Degrees north of the equator, between -90 and 90.
        latitude: f64,
        /// Degrees east of Greenwich, between -180 and 180.
        longitude: f64,
    },
    /// Resolve a city and ISO 3166-1 alpha-2 country code through Open-Meteo. This sends a
    /// geocoding request to Open-Meteo, which receives the machine's public IP address.
    Location {
        /// City and country code, written as `City, CC`.
        name: String,
    },
}

fn coordinate_alias(schema: &mut Schema) {
    let Some(branches) = schema
        .get_mut("oneOf")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    let Some(mut alias) = branches
        .iter()
        .find(|branch| {
            branch
                .pointer("/properties/at/const")
                .and_then(serde_json::Value::as_str)
                == Some("latlon")
        })
        .cloned()
    else {
        return;
    };
    let Some(tag) = alias.pointer_mut("/properties/at/const") else {
        return;
    };
    *tag = serde_json::Value::String("coordinates".to_owned());
    branches.push(alias);
}

/// Settings for the pager applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Pager {
    /// Whether each slot is a workspace or a window.
    pub mode: PagerMode,
    /// Whether a slot is drawn as a dot or as its label in a pill.
    pub shape: PagerShape,
    /// How much of the session the strip covers.
    pub scope: PagerScope,
    /// What a slot reads, when the shape is `labels`, and the fallback for every state below.
    /// Understands `{index}`, `{id}`, `{name}`, `{name-or-index}` and `{workspace-name}`.
    /// `{index}` falls back to the id, because only niri numbers its workspaces separately from
    /// their ids; in `windows` mode it is the slot's position and `{name}` is the window's app
    /// id, which is why `{workspace-name}` exists.
    pub label: String,
    /// The label for the slot the user is on, so the current workspace can show its name while
    /// the rest stay numbers. Same tokens as `label`; unset falls back to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_label: Option<String>,
    /// The label for every slot the user is not on. Same tokens as `label`; unset falls back
    /// to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfocused_label: Option<String>,
    /// The label for a slot asking for attention. Takes precedence over the other two, because a
    /// window wanting attention is the one thing the strip exists to surface. Same tokens as
    /// `label`; unset falls back to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urgent_label: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
/// What each slot stands for: `workspaces` for one slot per workspace, `windows` for one slot per
/// window on the current workspace.
#[serde(rename_all = "kebab-case")]
pub enum PagerMode {
    Workspaces,
    #[default]
    Windows,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
/// How a slot is drawn: `dots` for a dot each with the current one drawn longer, which takes
/// the least room on the bar, or `labels` for the slot's label in a pill.
#[serde(rename_all = "kebab-case")]
pub enum PagerShape {
    #[default]
    Dots,
    Labels,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
/// How much of the session the strip covers: `current` for only the workspace the user is on,
/// `output` for the workspaces on this panel's monitor, `session` for every workspace on every
/// monitor.
#[serde(rename_all = "kebab-case")]
pub enum PagerScope {
    Current,
    #[default]
    Output,
    Session,
}

impl Default for Pager {
    fn default() -> Self {
        Self {
            mode: PagerMode::default(),
            shape: PagerShape::default(),
            scope: PagerScope::default(),
            label: "{index}".to_owned(),
            focused_label: None,
            unfocused_label: None,
            urgent_label: None,
        }
    }
}

impl Default for Place {
    fn default() -> Self {
        Self::Here {}
    }
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            place: Place::default(),
            label: None,
            hours: 4,
            days: 6,
        }
    }
}

impl Default for NextEvent {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            within: 60,
            countdown: 60,
            horizon: 720,
            all_day: false,
            upcoming: 5,
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            tooltip_format: None,
            label_format: "%a, %-d %b, %H:%M".to_owned(),
            timezone: None,
            week_numbers: true,
            timezones: Vec::new(),
            hide_all_day: false,
        }
    }
}

impl From<Kind> for Applet {
    fn from(kind: Kind) -> Self {
        Self {
            kind,
            regional: super::Regional::default(),
        }
    }
}

impl Applet {
    pub fn from_name(name: &str) -> Option<Self> {
        let mut table = toml::Table::new();
        table.insert("extends".to_owned(), toml::Value::String(name.to_owned()));
        Kind::deserialize(table).ok().map(Self::from).or_else(|| {
            is_desktop_id(name).then(|| {
                Self::from(Kind::Exec(Box::new(Exec {
                    applet: name.to_owned(),
                    options: serde_json::Map::new(),
                    env: BTreeMap::new(),
                })))
            })
        })
    }
}

pub fn is_desktop_id(name: &str) -> bool {
    let mut segments = name.split('.');
    let valid = |segment: &str| {
        let mut bytes = segment.bytes();
        bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    };
    segments.next().is_some_and(valid) && segments.next().is_some_and(valid) && segments.all(valid)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<String, Applet>, D::Error>
where
    D: Deserializer<'de>,
{
    BTreeMap::<String, toml::Table>::deserialize(deserializer)?
        .into_iter()
        .map(|(name, table)| {
            entry(&name, table)
                .map(|applet| (name.clone(), applet))
                .map_err(|error| D::Error::custom(format!("[applets.{name}]: {error}")))
        })
        .collect()
}

fn entry(name: &str, mut table: toml::Table) -> Result<Applet, toml::de::Error> {
    if !table.contains_key("extends") && is_desktop_id(name) {
        table.insert("extends".to_owned(), toml::Value::String("exec".to_owned()));
        table
            .entry("applet")
            .or_insert_with(|| toml::Value::String(name.to_owned()));
    } else {
        table
            .entry("extends")
            .or_insert_with(|| toml::Value::String(name.to_owned()));
    }
    let mut kind = Kind::deserialize(table)?;
    if let Kind::Exec(exec) = &kind
        && !is_desktop_id(&exec.applet)
    {
        return Err(toml::de::Error::custom("applet must be a desktop-file id"));
    }
    if let Kind::Exec(exec) = &kind {
        exec.validate().map_err(toml::de::Error::custom)?;
    }
    on_earth(&mut kind)?;
    runnable(&kind)?;
    thresholds(&kind)?;
    Ok(Applet {
        kind,
        regional: super::Regional::default(),
    })
}

/// The wire refuses these too, but a document saying so at load names the table and the key rather
/// than failing a `WatchPlace` call nobody is making.
fn on_earth(kind: &mut Kind) -> Result<(), toml::de::Error> {
    let Kind::Weather(weather) = kind else {
        return Ok(());
    };
    match &mut weather.place {
        Place::Coordinates {
            latitude,
            longitude,
        } => {
            if !(-90.0..=90.0).contains(latitude) {
                return Err(toml::de::Error::custom(
                    "latitude is degrees north of the equator, between -90 and 90",
                ));
            }
            if !(-180.0..=180.0).contains(longitude) {
                return Err(toml::de::Error::custom(
                    "longitude is degrees east of Greenwich, between -180 and 180",
                ));
            }
        }
        Place::Location { name } => {
            let Some((city, country_code)) = name.rsplit_once(',') else {
                return Err(toml::de::Error::custom(
                    "location must be written as `City, CC`",
                ));
            };
            let city = city.trim();
            let country_code = country_code.trim();
            if city.is_empty()
                || city.chars().count() > 100
                || country_code.len() != 2
                || !country_code.bytes().all(|byte| byte.is_ascii_uppercase())
            {
                return Err(toml::de::Error::custom(
                    "location must be written as `City, CC`, with a city of at most 100 characters and a two-letter uppercase country code",
                ));
            }
            *name = format!("{city}, {country_code}");
        }
        Place::Here {} => {}
    }
    Ok(())
}

fn chips<'de, D>(deserializer: D) -> Result<Vec<Chip>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Vec::<Chip>::deserialize(deserializer)?;
    let mut seen = std::collections::HashSet::new();
    Ok(raw.into_iter().filter(|chip| seen.insert(*chip)).collect())
}

/// `warn-percent` above `critical-percent` would make a reading skip straight from `None` to
/// `Severity::Error` with no `Warning` in between — a document saying so at load names the table
/// rather than leaving the popover to render a threshold ordering nobody chose.
fn thresholds(kind: &Kind) -> Result<(), toml::de::Error> {
    let Kind::SystemMonitor(settings) = kind else {
        return Ok(());
    };
    if settings.warn_percent > settings.critical_percent {
        return Err(toml::de::Error::custom(
            "warn-percent must be less than or equal to critical-percent",
        ));
    }
    Ok(())
}

/// Resolves one zone entry the same way every panel zone resolves a name: its own table entry
/// first, `Applet::from_name` fallback second. The single source of truth behind both the panel's
/// own `applets::configured` and a service's demand-gating `placed_kinds` — kept in one place so
/// the two cannot drift apart.
pub fn resolve_applet(name: &str, applets: &BTreeMap<String, Applet>) -> Option<Applet> {
    applets
        .get(name)
        .cloned()
        .or_else(|| Applet::from_name(name))
}

fn runnable(kind: &Kind) -> Result<(), toml::de::Error> {
    let Kind::Command(command) = kind else {
        return Ok(());
    };
    let gestures = [
        ("on-click", &command.on_click),
        ("on-middle-click", &command.on_middle_click),
        ("on-right-click", &command.on_right_click),
        ("on-scroll-up", &command.on_scroll_up),
        ("on-scroll-down", &command.on_scroll_down),
        ("on-scroll-left", &command.on_scroll_left),
        ("on-scroll-right", &command.on_scroll_right),
    ];
    for (key, argv) in gestures {
        names_a_program(key, argv)?;
    }
    if let Some(icon) = command.icon.as_deref()
        && icon.contains('/')
        && !icon.starts_with('/')
    {
        return Err(toml::de::Error::custom(
            "icon is a theme name or an absolute path to an image; a relative path would resolve \
             against wherever the panel happened to start",
        ));
    }
    Ok(())
}

fn names_a_program(key: &str, argv: &[String]) -> Result<(), toml::de::Error> {
    if argv
        .first()
        .is_some_and(|program| program.trim().is_empty())
    {
        return Err(toml::de::Error::custom(format!(
            "{key} names no program: its first element is what runs, and the rest are that \
             program's arguments"
        )));
    }
    Ok(())
}

pub fn schema(generator: &mut SchemaGenerator) -> Schema {
    let aliased = Kind::json_schema(generator);
    let mut properties = serde_json::Map::new();
    let mut desktop_id = None;

    if let Some(branches) = aliased.get("oneOf").and_then(serde_json::Value::as_array) {
        for branch in branches {
            let Some(name) = branch
                .pointer("/properties/extends/const")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let mut by_key = branch.clone();
            if let Some(required) = by_key.get_mut("required") {
                *required = serde_json::Value::Array(
                    required
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|value| value.as_str() != Some("extends"))
                        .cloned()
                        .collect(),
                );
            }
            properties.insert(name.to_owned(), by_key);
            if name == "exec" {
                let mut by_id = branch.clone();
                by_id["required"] = serde_json::json!([]);
                desktop_id = Some(by_id);
            }
        }
    }

    let mut schema = serde_json::Map::new();
    schema.insert(
        "type".to_owned(),
        serde_json::Value::String("object".to_owned()),
    );
    schema.insert(
        "properties".to_owned(),
        serde_json::Value::Object(properties),
    );
    if let Some(desktop_id) = desktop_id {
        schema.insert(
            "patternProperties".to_owned(),
            serde_json::json!({
                "^[_A-Za-z][_A-Za-z0-9-]*\\.[_A-Za-z][_A-Za-z0-9-]*(\\.[_A-Za-z][_A-Za-z0-9-]*)*$": desktop_id
            }),
        );
    }
    schema.insert("additionalProperties".to_owned(), aliased.to_value());
    Schema::from(schema)
}

impl Kind {
    pub fn tooltip_format(&self) -> Option<&str> {
        match self {
            Self::Audio(kind) => kind.tooltip_format.as_deref(),
            Self::Power(kind) => kind.tooltip_format.as_deref(),
            Self::Bluetooth(kind) => kind.tooltip_format.as_deref(),
            Self::Clipboard(kind) => kind.tooltip_format.as_deref(),
            Self::Clock(kind) => kind.tooltip_format.as_deref(),
            Self::Command(kind) => kind.tooltip_format.as_deref(),
            Self::Display(kind) => kind.tooltip_format.as_deref(),
            Self::Kdeconnect(kind) => kind.tooltip_format.as_deref(),
            Self::Mpris(kind) => kind.tooltip_format.as_deref(),
            Self::Network(kind) => kind.tooltip_format.as_deref(),
            Self::NextEvent(kind) => kind.tooltip_format.as_deref(),
            Self::Notifications(kind) => kind.tooltip_format.as_deref(),
            Self::Places(kind) => kind.tooltip_format.as_deref(),
            Self::Printing(kind) => kind.tooltip_format.as_deref(),
            Self::Privacy(kind) => kind.tooltip_format.as_deref(),
            Self::Removable(kind) => kind.tooltip_format.as_deref(),
            Self::SystemMonitor(kind) => kind.tooltip_format.as_deref(),
            Self::Weather(kind) => kind.tooltip_format.as_deref(),
            Self::ColorPicker { tooltip_format }
            | Self::Idle { tooltip_format }
            | Self::Keyboard { tooltip_format }
            | Self::Ruler { tooltip_format }
            | Self::Session { tooltip_format }
            | Self::WorkspaceName { tooltip_format } => tooltip_format.as_deref(),
            Self::Exec(_)
            | Self::Heartbeat {}
            | Self::Pager(_)
            | Self::Tray(_)
            | Self::WidgetDemo {} => None,
        }
    }
}

#[cfg(test)]
mod tests;
