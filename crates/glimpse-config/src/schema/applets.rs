use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// One applet on a bar and the settings its kind understands.
/// Use `extends` to run another copy, such as `[applets.clock-utc]` with `extends = "clock"`.
/// Each copy starts from its kind's defaults, without inheriting another applet table's settings.
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
    /// Speaker and application volume.
    Audio(Audio),
    /// Battery, power mode, keep awake and peripheral batteries in its popover.
    Power(Power),
    /// Bluetooth and paired devices.
    Bluetooth(Bluetooth),
    /// Clipboard history.
    Clipboard(Clipboard),
    /// The time and date, with a calendar in its popover.
    Clock(Clock),
    /// Picks a color from the screen on right click; the popover keeps the picked colors.
    /// Your own commands on the bar.
    Command(Box<Command>),
    /// Displays, brightness and night light.
    Display(Display),
    /// A third-party applet.
    Exec(Box<Exec>),
    /// A counter that ticks once a second. A development fixture: it proves the service is
    /// reachable and events are arriving, and is not meant for a real bar.
    Heartbeat {},
    /// Keep your screen awake.
    Idle {
        /// Your tooltip text, with `{status}` for what is keeping your screen awake.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schemars(extend("default" = "{status}"))]
        tooltip_format: Option<String>,
    },
    /// Your paired KDE Connect phone.
    /// The active keyboard layout, and switches between the configured ones.
    Keyboard {
        /// Your tooltip text, with `{code}` for the short layout label and `{name}` for its full
        /// name.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schemars(extend("default" = "{name}"))]
        tooltip_format: Option<String>,
    },
    /// Music and playback controls.
    Mpris(Mpris),
    /// Your network connections.
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
    /// Microphone, camera, screen and location use.
    Privacy(Privacy),
    /// Removable drives.
    Removable(Removable),
    /// Screen measurements.
    /// Log out, suspend, restart and shut down.
    Session {
        /// Your tooltip text, with `{user}` for your user name.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[schemars(extend("default" = "Session"))]
        tooltip_format: Option<String>,
    },
    /// Your computer's resource usage.
    /// Application icons in the system tray.
    Tray(Tray),
    /// Current weather and forecast.
    Weather(Weather),
    /// A widget preview for testing.
    WidgetDemo {},
    /// Your current workspace's name.
    WorkspaceName {
        /// Your tooltip text, using the same placeholders as the pager's `label`.
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
    /// Hide tray icons by their application id.
    pub hide: Vec<String>,
    /// Keep these tray icons on your bar in the order you give, even beyond `max-visible`.
    pub pin: Vec<String>,
    /// How many tray icons stay on your bar before the rest go behind an arrow. Pinned icons always
    /// stay visible; `0` shows every icon.
    pub max_visible: u8,
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
    /// Text beside your icon.
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
    /// What scrolling up runs, once per notch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_scroll_up: Vec<String>,
    /// What scrolling down runs, once per notch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_scroll_down: Vec<String>,
    /// What scrolling left runs, once per notch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_scroll_left: Vec<String>,
    /// What scrolling right runs, once per notch.
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
    /// Text beside your icon, with `{count}` for the number of entries in your history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_format: Option<String>,
    /// How many recent entries the popover shows. Pinned entries don't count.
    pub visible: usize,
    /// How many characters of each clipboard entry you see in its preview, from 8 to 128.
    pub preview_chars: usize,
    /// Keep the chip visible when your history is empty.
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
    /// Your tooltip text, with `{count}` for the number of unread notifications.
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

/// Settings for the mpris applet. Which players exist and which one is current is the service's
/// decision, in `[mpris]`; this is only how the bar renders the one it is given.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Mpris {
    /// Your tooltip text, using the same placeholders as `label-format`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// Your track label, using `{player}`, `{title}`, `{artist}`, `{album}`, `{state}`,
    /// `{position}`, `{duration}` and `{remaining}`.
    pub label_format: String,
    /// How many characters of your track label you see before it ends in `…`.
    pub max_length: u8,
}

/// The network applet's own settings. Where the networks are scanned and listed is `[network]`;
/// this is only what the chip on the bar does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Network {
    /// Your tooltip text, with `{connection}` for your network, `{metered}` for its metered status,
    /// `{signal}` for Wi-Fi strength and `{vpn}` for your VPN's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = NETWORK_TOOLTIP))]
    pub tooltip_format: Option<String>,
    /// How many other networks you see before a "more networks" row; `0` shows all. Connected and
    /// saved networks don't count.
    pub visible_networks: usize,
    /// Show your active VPN as a separate icon beside your network icon.
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
    /// Your clock's time zone, such as `UTC` or `Asia/Tokyo`. Leave it unset to use your local
    /// time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Show today's week number above your calendar.
    pub week_numbers: bool,
    /// The other zones the popover lists under its world clock. Empty hides the section.
    pub timezones: Vec<Timezone>,
    /// Hide all-day events from your calendar's day list and month grid.
    pub hide_all_day: bool,
}

/// The bluetooth applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Bluetooth {
    /// Your tooltip text, with `{status}` for connection status, `{devices}` for connected devices
    /// and `{adapter}` for your Bluetooth adapter's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "{status}"))]
    pub tooltip_format: Option<String>,
    /// How many disconnected paired devices you see before a "more devices" row. Connected devices
    /// don't count.
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
    /// Your tooltip text, with `{bookmarks}` for your bookmark count and `{places}` for your home
    /// and user-directory count.
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
    /// Your tooltip text, with `{document}` for the current document, `{printer}` for its printer,
    /// `{pages}` for progress and `{count}` for the jobs shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = PRINTING_TOOLTIP))]
    pub tooltip_format: Option<String>,
    /// How many active print jobs you see.
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
    /// Show whenever a printer exists.
    Always,
    /// Show while a print job is active.
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
    /// Your tooltip text, with `{name}` for the display's name and `{brightness}` for its
    /// brightness percentage.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "{name}: {brightness}%"))]
    pub tooltip_format: Option<String>,
    /// How much scrolling one notch changes your display's brightness, in percent, from 1 to 100.
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
    /// The time zone for this row, such as `Asia/Tokyo`.
    pub timezone: String,
    /// A note beside the time, such as `the office`.
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
    /// How many minutes before an event starts it appears on your bar. With `0`, you only see
    /// events already under way.
    pub within: u64,
    /// How many minutes before an event starts you see a countdown beside its title. With `0`, you
    /// only see a countdown after the event starts.
    pub countdown: u64,
    /// How many minutes ahead your event list reaches, with `within` as the minimum.
    pub horizon: u64,
    /// Show all-day events on your bar when there are no timed events to show.
    pub all_day: bool,
    /// How many upcoming events you see below the current one.
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
    /// Your place's name in the tooltip and weather panel. Leave it unset to use the city name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// How many forecast hours you see, starting with the next hour.
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
    /// Your current location.
    Here {},
    /// A fixed location.
    #[serde(rename = "latlon", alias = "coordinates")]
    Coordinates {
        /// Degrees north of the equator, between -90 and 90.
        latitude: f64,
        /// Degrees east of Greenwich, between -180 and 180.
        longitude: f64,
    },
    /// A city and country.
    Location {
        /// Your city and two-letter uppercase country code, such as `Amsterdam, NL`.
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
    /// Your slot text, using `{index}`, `{id}`, `{name}`, `{name-or-index}` and `{workspace-name}`.
    /// This also supplies any state label you leave unset.
    pub label: String,
    /// Your current slot's text, using the same placeholders as `label`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_label: Option<String>,
    /// Text for other slots, using the same placeholders as `label`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfocused_label: Option<String>,
    /// Text for a slot asking for attention, using the same placeholders as `label`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urgent_label: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
/// Choose `workspaces` for one slot per workspace, or `windows` for windows on each in-scope workspace active on its output.
/// In `windows` mode, scroll vertically between windows and horizontally between workspaces; `workspaces` reverses those axes.
/// Your slot tooltip shows the window title in `windows` mode and the workspace name or number otherwise.
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

/// Applets that used to be built in and now ship as exec applets. Naming one of them in `extends`
/// is the one legacy spelling this file still sees often enough to answer by hand, since the
/// generated variant list says only what the kinds are called now.
const EXTRACTED: [(&str, &str); 4] = [
    ("system-monitor", "me.aresa.SystemMonitor"),
    ("color-picker", "me.aresa.ColorPicker"),
    ("ruler", "me.aresa.Ruler"),
    ("kdeconnect", "me.aresa.Kdeconnect"),
];

fn extracted(name: &str, table: &toml::Table) -> Option<String> {
    let extends = table.get("extends")?.as_str()?;
    let applet = EXTRACTED
        .into_iter()
        .find(|(kind, _)| *kind == extends)
        .map(|(_, applet)| applet)?;
    Some(format!(
        "{name} is an exec applet now: write extends = \"exec\" and applet = \"{applet}\""
    ))
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
    if let Some(reason) = extracted(name, &table) {
        return Err(toml::de::Error::custom(reason));
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
            Self::Mpris(kind) => kind.tooltip_format.as_deref(),
            Self::Network(kind) => kind.tooltip_format.as_deref(),
            Self::NextEvent(kind) => kind.tooltip_format.as_deref(),
            Self::Notifications(kind) => kind.tooltip_format.as_deref(),
            Self::Places(kind) => kind.tooltip_format.as_deref(),
            Self::Printing(kind) => kind.tooltip_format.as_deref(),
            Self::Privacy(kind) => kind.tooltip_format.as_deref(),
            Self::Removable(kind) => kind.tooltip_format.as_deref(),
            Self::Weather(kind) => kind.tooltip_format.as_deref(),
            Self::Idle { tooltip_format }
            | Self::Keyboard { tooltip_format }
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
