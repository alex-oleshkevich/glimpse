use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// One applet on a bar: the settings every applet understands, and the ones its own kind does.
/// The table name is the applet's name, and `extends` says which kind it is when the two differ —
/// which is how one kind can appear more than once, as `[applets.clock-utc]` with
/// `extends = "clock"`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Applet {
    #[serde(flatten)]
    pub common: Common,
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

/// The settings every applet understands, whatever kind it is. They sit in the same table as the
/// kind's own settings; nothing nests them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Common {
    /// What the bar's tooltip reads. The tokens are the applet's own — `strftime` for the clock,
    /// `{index}` and `{name}` for the pager, `{summary}`, `{detail}`, `{when}` and `{conflicts}`
    /// for next-event — the same way `label` already differs between them. A token with nothing
    /// behind it renders as nothing. Unset means the applet shows no tooltip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip_format: Option<String>,
    /// The label on the row the applet's popover puts in its footer. Set it together with
    /// `settings-command`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_label: Option<String>,
    /// The program that row runs, as a command and its arguments:
    /// `["xdg-open", "https://calendar.google.com/"]`. It is a list rather than one string because
    /// there is no shell between here and the program — an argument containing a space is one
    /// element, and nothing has to be quoted, escaped or protected from word splitting.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub settings_command: Vec<String>,
}

/// Which kind of applet this is, and the settings that kind alone understands.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(
    tag = "extends",
    rename_all = "kebab-case",
    rename_all_fields = "kebab-case",
    deny_unknown_fields
)]
pub enum Kind {
    /// Output volume, with the default sink and per-application streams in its popover.
    Audio {},
    /// Charge level and time remaining, with the power profile in its popover.
    Battery(Battery),
    /// Adapter state and paired devices.
    Bluetooth(Bluetooth),
    /// Display backlight level, and the keyboard's own where the machine has one.
    Brightness(Brightness),
    /// Clipboard history.
    Clipboard(Clipboard),
    /// The time and date, with a calendar in its popover.
    Clock(Clock),
    /// The latest picked color. A left click opens the lens to pick one from the screen, a right
    /// click the palette of recent picks in every notation. Needs `glimpse-picker`.
    ColorPicker {},
    /// A chip the user defines, running a program on a click or a scroll.
    Command(Box<Command>),
    /// Connected outputs, their modes and their arrangement.
    Display {},
    /// Hosts a third-party applet binary that draws its own popover.
    Exec {},
    /// A counter that ticks once a second. A development fixture: it proves the service is
    /// reachable and events are arriving, and is not meant for a real bar.
    Heartbeat {},
    /// Idle inhibition, for keeping the screen awake.
    Idle {},
    /// A phone paired through KDE Connect: its battery on the bar, and ring, ping, send and pair
    /// in its popover. Needs `kdeconnectd` running; it is never started from here.
    Kdeconnect(Kdeconnect),
    /// The active keyboard layout, and switches between the configured ones.
    Keyboard {},
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
    /// Removable drives and their volumes, with mount, unmount and eject in its popover.
    Removable(Removable),
    /// Measures on-screen pixel distances. A left click opens the lens to measure from the
    /// screen. Needs `glimpse-ruler`.
    Ruler {},
    /// Log out, suspend, restart and shut down.
    Session {},
    /// Live CPU, RAM, swap, disk, network and (amdgpu) GPU load. Its backing service samples
    /// nothing at all unless this kind is actually placed on a panel — see `placed_kinds`.
    SystemMonitor(SystemMonitor),
    /// The system tray: icons from applications that ask for one.
    Tray(Tray),
    /// Current conditions, with the forecast in its popover.
    Weather(Weather),
    /// The current workspace's name, renamed from its popover.
    WorkspaceName {},
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
    /// Chips shown on the bar, in order. Anything sampled but left out here still appears in the
    /// popover — this list is panel real estate, not visibility. A repeated chip collapses to its
    /// first occurrence.
    #[serde(deserialize_with = "chips")]
    pub chips: Vec<Chip>,
    /// How each chip's own label reads. `{name}` is the chip's own localized name ("CPU", "RAM",
    /// …); `{value}` is its reading ("42%" for a percentage chip, "↓1.2 MB/s" for network). An
    /// unrecognized token is left as literal text rather than silently emptied.
    pub chip_format: String,
    /// Percent at and above which any percentage reading (CPU, RAM, swap, GPU usage/memory) turns
    /// `Severity::Warning`. Network has no percentage and is never colored.
    #[schemars(range(min = 1, max = 100))]
    pub warn_percent: u8,
    /// Percent at and above which a reading turns `Severity::Error`. This codebase's `Severity`
    /// enum has no "Critical" variant — it is `Info`/`Warning`/`Error` only.
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
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self {
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
pub struct Battery {
    pub indicator_style: BatteryIndicatorStyle,
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

impl Default for Battery {
    fn default() -> Self {
        Self {
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
    /// What the bar shows. Placeholders are replaced by name: `{player}`, `{title}`, `{artist}`,
    /// `{album}`, `{state}`, `{position}`, `{duration}` and `{remaining}`. A placeholder with
    /// nothing behind it renders as nothing.
    pub label_format: String,
    /// Longest label the bar shows, in characters; the rest is ellipsized. Track titles come from
    /// whatever is playing and are unbounded, and a bar that resizes as they change is worse than
    /// one that truncates.
    pub max_length: u8,
    /// Whether the popover lists the players other than the one on the bar.
    pub show_others: bool,
    /// Whether the popover shows album art.
    pub show_art: bool,
}

/// The network applet's own settings. Where the networks are scanned and listed is `[network]`;
/// this is only what the chip on the bar does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Network {
    /// How many networks the popover lists before the rest go behind a "more networks" row. `0`
    /// lists every network in range.
    pub visible_networks: usize,
    /// Whether an active VPN gets a chip of its own beside the network chip. An indicator is an
    /// icon, and two states overlaid on one glyph are illegible at bar size.
    pub vpn_chip: bool,
    /// Whether a metered connection is named in the tooltip. It is always marked on the row inside
    /// the popover; this is only about the bar.
    pub metered_in_tooltip: bool,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            visible_networks: 8,
            vpn_chip: true,
            metered_in_tooltip: true,
        }
    }
}

impl Default for Mpris {
    fn default() -> Self {
        Self {
            label_format: "{title} — {artist}".to_owned(),
            max_length: 40,
            show_others: true,
            show_art: true,
        }
    }
}

/// Settings for the clock applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Clock {
    /// A `strftime` format string for the bar, such as `%H:%M` or `%a %d %b %H:%M`.
    #[serde(alias = "format")]
    pub label_format: String,
    /// The IANA zone this clock reads, such as `UTC` or `Asia/Tokyo`. Unset is the local zone;
    /// naming one is how a second `[applets.clock-utc]` shows somewhere else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Which day a calendar week starts on.
    pub first_day: FirstDay,
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
    /// How many paired devices the popover lists before the rest go behind a drawer.
    pub devices: usize,
    /// How many nearby devices the popover lists while scanning.
    pub nearby: usize,
}

impl Default for Bluetooth {
    fn default() -> Self {
        Self {
            devices: 6,
            nearby: 8,
        }
    }
}

/// The places applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Places {
    /// How many bookmarks the popover lists before the rest go behind a drawer.
    pub bookmarks: usize,
}

impl Default for Places {
    fn default() -> Self {
        Self { bookmarks: 8 }
    }
}

/// The removable applet. How often free space is sampled is `[removable] capacity-interval`; this
/// is only how the bar and its popover behave.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Removable {
    /// How many drives the popover lists before the rest go behind a drawer.
    pub volumes: usize,
}

impl Default for Removable {
    fn default() -> Self {
        Self { volumes: 6 }
    }
}

/// The printing applet's own settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Printing {
    /// Print jobs listed in the popover before older ones are hidden.
    pub jobs: usize,
}

impl Default for Printing {
    fn default() -> Self {
        Self { jobs: 8 }
    }
}

/// The privacy applet's own settings: which of its sources it shows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Privacy {
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
            show_camera: true,
            show_microphone: true,
            show_screencast: true,
            show_location: true,
        }
    }
}

/// Settings for the brightness applet. The floor a display can reach is `[brightness] minimum`;
/// this is only how the bar and its popover behave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Brightness {
    /// How much a scroll notch on the chip moves the current display, in percent of its range.
    /// Converted to native units at the call site and rounded away from zero, so a notch always
    /// moves something even on a display with a small native range. Zero would leave the wheel
    /// silently dead; the range is 1 to 100.
    #[serde(deserialize_with = "scroll_percent")]
    #[schemars(range(min = 1, max = 100))]
    pub scroll_step: u8,
    /// Whether the keyboard's own backlight, where the machine has one, gets a fader in the
    /// popover alongside the displays.
    pub show_keyboard: bool,
}

impl Default for Brightness {
    fn default() -> Self {
        Self {
            scroll_step: 5,
            show_keyboard: true,
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

/// Which day a calendar week starts on. There is deliberately no `locale` here, unlike
/// `hour-format`: the two ways to ask the system are GTK's translated `calendar:week_start:0`,
/// which came back untranslated when it was measured and would have answered Sunday under an
/// `LC_TIME` that means Monday, and glibc's `_NL_TIME_FIRST_WEEKDAY`, which is not portable. A
/// `locale` that quietly answers wrong is worse than a default that says what it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FirstDay {
    #[default]
    Monday,
    Sunday,
}

/// Settings for the next-event applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct NextEvent {
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
    /// Which place this applet shows.
    pub place: Place,
    /// What the bar and the popover call this place. Unset means a fixed place reads as the pair
    /// it was given, and `here` as whatever the geolocation service resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// How many hours the popover's strip shows. The strip starts at the next hour — the hour
    /// standing is already the hero.
    pub hours: u8,
    /// How many days the popover's list shows. The list starts at tomorrow — today is already
    /// the hero, the strip and the details page, and repeating it as a row says nothing new.
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
    #[default]
    Workspaces,
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
            label_format: "%a, %-d %b, %H:%M".to_owned(),
            timezone: None,
            first_day: FirstDay::default(),
            week_numbers: true,
            timezones: Vec::new(),
            hide_all_day: false,
        }
    }
}

const COMMON: [&str; 3] = ["tooltip-format", "settings-label", "settings-command"];

impl Common {
    pub fn settings(&self) -> Option<(&str, &[String])> {
        let label = self.settings_label.as_deref()?;
        Some((label, self.settings_command.as_slice()))
    }
}

impl From<Kind> for Applet {
    fn from(kind: Kind) -> Self {
        Self {
            common: Common::default(),
            kind,
            regional: super::Regional::default(),
        }
    }
}

impl Applet {
    pub fn from_name(name: &str) -> Option<Self> {
        let mut table = toml::Table::new();
        table.insert("extends".to_owned(), toml::Value::String(name.to_owned()));
        Kind::deserialize(table).ok().map(Self::from)
    }
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
    let common = take_common(&mut table)?;
    table
        .entry("extends")
        .or_insert_with(|| toml::Value::String(name.to_owned()));
    let keys: Vec<String> = table.keys().cloned().collect();
    let mut kind =
        Kind::deserialize(table).map_err(|error| name_the_common_settings(error, &keys))?;
    on_earth(&mut kind)?;
    runnable(&kind)?;
    thresholds(&kind)?;
    Ok(Applet {
        common,
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

fn name_the_common_settings(error: toml::de::Error, keys: &[String]) -> toml::de::Error {
    let message = error.to_string();
    let message = message.trim_end();
    let Some(field) = message
        .strip_prefix("unknown field `")
        .and_then(|rest| rest.split('`').next())
    else {
        return error;
    };
    if !keys.iter().any(|key| key == field) {
        return error;
    }
    let common = COMMON.map(|key| format!("`{key}`")).join(", ");
    toml::de::Error::custom(format!("{message}, or one of {common}"))
}

fn take_common(table: &mut toml::Table) -> Result<Common, toml::de::Error> {
    let mut taken = toml::Table::new();
    for key in COMMON {
        if let Some(value) = table.remove(key) {
            taken.insert(key.to_owned(), value);
        }
    }

    let common = Common::deserialize(taken)?;
    if common.settings_label.is_some() == common.settings_command.is_empty() {
        return Err(toml::de::Error::custom(
            "settings-label and settings-command are set together: a label with no command is a \
             row that does nothing, and a command with no label is a row nobody can see",
        ));
    }
    names_a_program("settings-command", &common.settings_command)?;
    Ok(common)
}

pub fn schema(generator: &mut SchemaGenerator) -> Schema {
    let aliased = with_common(generator);
    let mut properties = serde_json::Map::new();

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
    schema.insert("additionalProperties".to_owned(), aliased.to_value());
    Schema::from(schema)
}

fn with_common(generator: &mut SchemaGenerator) -> Schema {
    let shared = Common::json_schema(generator)
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut kinds = Kind::json_schema(generator);
    if let Some(branches) = kinds
        .get_mut("oneOf")
        .and_then(serde_json::Value::as_array_mut)
    {
        for branch in branches {
            let Some(properties) = branch
                .get_mut("properties")
                .and_then(serde_json::Value::as_object_mut)
            else {
                continue;
            };
            for (key, value) in &shared {
                properties.insert(key.clone(), value.clone());
            }
        }
    }
    kinds
}

#[cfg(test)]
mod tests {
    use super::{
        BatteryIndicatorStyle, COMMON, Chip, Common, Kind, NotificationIndicatorStyle, Printing,
        Privacy,
    };

    #[test]
    fn every_common_setting_is_taken_off_the_table() {
        let schema = schemars::schema_for!(Common);
        let declared: Vec<String> = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .expect("Common is an object")
            .keys()
            .cloned()
            .collect();

        for key in &declared {
            assert!(
                COMMON.contains(&key.as_str()),
                "`{key}` is a common setting the splitter never removes, so it reaches the kind \
                 and is refused as one of its own"
            );
        }
        assert_eq!(
            COMMON.len(),
            declared.len(),
            "the splitter removes a key no common setting declares"
        );
    }

    #[test]
    fn places_still_resolves_from_a_bare_name_and_reads_its_own_settings() {
        let bare = super::Applet::from_name("places").expect("a known applet name");
        assert_eq!(bare.kind, Kind::Places(super::Places::default()));

        let configured: crate::Config =
            toml::from_str("[applets.pl]\nextends = \"places\"\nbookmarks = 4\n")
                .expect("the table loads");
        assert_eq!(
            configured.applets["pl"].kind,
            Kind::Places(super::Places { bookmarks: 4 })
        );
    }

    #[test]
    fn removable_still_resolves_from_a_bare_name_and_reads_its_own_settings() {
        let bare = super::Applet::from_name("removable").expect("a known applet name");
        assert_eq!(bare.kind, Kind::Removable(super::Removable::default()));

        let configured: crate::Config =
            toml::from_str("[applets.rm]\nextends = \"removable\"\nvolumes = 4\n")
                .expect("the table loads");
        assert_eq!(
            configured.applets["rm"].kind,
            Kind::Removable(super::Removable { volumes: 4 })
        );
    }

    #[test]
    fn kdeconnect_resolves_from_a_bare_name_and_reads_its_own_settings() {
        let bare = super::Applet::from_name("kdeconnect").expect("a known applet name");
        assert_eq!(bare.kind, Kind::Kdeconnect(super::Kdeconnect::default()));

        let configured: crate::Config = toml::from_str(
            "[applets.phone]\nextends = \"kdeconnect\"\nindicator-style = \"icon-only\"\n\
             label-format = \"{name}\"\ndevice = \"Pixel 8\"\nhide-when-disconnected = true\n",
        )
        .expect("the table loads");
        assert_eq!(
            configured.applets["phone"].kind,
            Kind::Kdeconnect(super::Kdeconnect {
                indicator_style: BatteryIndicatorStyle::IconOnly,
                label_format: "{name}".to_owned(),
                device: Some("Pixel 8".to_owned()),
                hide_when_disconnected: true,
            })
        );
        assert!(
            toml::from_str::<crate::Config>("[applets.p]\nextends = \"kdeconnect\"\nbattery = 1\n")
                .is_err(),
            "an unknown key under kdeconnect is refused"
        );
    }

    #[test]
    fn bluetooth_still_resolves_from_a_bare_name_and_reads_its_own_settings() {
        let bare = super::Applet::from_name("bluetooth").expect("a known applet name");
        assert_eq!(bare.kind, Kind::Bluetooth(super::Bluetooth::default()));

        let configured: crate::Config =
            toml::from_str("[applets.bt]\nextends = \"bluetooth\"\ndevices = 4\n")
                .expect("the table loads");
        assert_eq!(
            configured.applets["bt"].kind,
            Kind::Bluetooth(super::Bluetooth {
                devices: 4,
                nearby: 8,
            })
        );
    }

    #[test]
    fn notification_indicator_style_defaults_to_a_dot_and_reads_every_variant() {
        let default: crate::Config = toml::from_str("[applets.notifications]\n")
            .expect("the built-in notification applet loads");
        let Kind::Notifications(default) = &default.applets["notifications"].kind else {
            panic!("notifications resolves to its own kind");
        };
        assert_eq!(default.indicator_style, NotificationIndicatorStyle::IconDot);

        for (value, expected) in [
            ("icon-only", NotificationIndicatorStyle::IconOnly),
            ("icon-dot", NotificationIndicatorStyle::IconDot),
            ("icon-counter", NotificationIndicatorStyle::IconCounter),
        ] {
            let text = format!("[applets.notifications]\nindicator-style = \"{value}\"\n");
            let document: crate::Config = toml::from_str(&text).expect("the style is valid");
            let Kind::Notifications(settings) = &document.applets["notifications"].kind else {
                panic!("notifications resolves to its own kind");
            };
            assert_eq!(settings.indicator_style, expected);
        }

        toml::from_str::<crate::Config>("[applets.notifications]\nindicator-style = \"counter\"\n")
            .expect_err("an undocumented spelling is refused");
    }

    #[test]
    fn battery_defaults_to_icon_only_and_reads_every_style() {
        let default: crate::Config =
            toml::from_str("[applets.battery]\n").expect("the built-in battery applet loads");
        let Kind::Battery(default) = &default.applets["battery"].kind else {
            panic!("battery resolves to its own kind");
        };
        assert_eq!(default.indicator_style, BatteryIndicatorStyle::IconOnly);
        assert_eq!(default.label_format, "{percentage}");

        for (value, expected) in [
            ("icon-only", BatteryIndicatorStyle::IconOnly),
            ("icon-text", BatteryIndicatorStyle::IconText),
            ("text", BatteryIndicatorStyle::Text),
        ] {
            let text = format!("[applets.battery]\nindicator-style = \"{value}\"\n");
            let document: crate::Config = toml::from_str(&text).expect("the style is valid");
            let Kind::Battery(settings) = &document.applets["battery"].kind else {
                panic!("battery resolves to its own kind");
            };
            assert_eq!(settings.indicator_style, expected);
        }

        toml::from_str::<crate::Config>("[applets.battery]\nindicator-style = \"percent\"\n")
            .expect_err("an undocumented spelling is refused");
        toml::from_str::<crate::Config>("[applets.battery]\nunknown = 1\n")
            .expect_err("a struct variant refuses keys that are not its own");
    }

    #[test]
    fn a_repeated_chip_collapses_to_its_first_occurrence() {
        let document: crate::Config =
            toml::from_str("[applets.system-monitor]\nchips = [\"cpu\", \"cpu\", \"ram\"]\n")
                .expect("chips is a key of this table");
        let Kind::SystemMonitor(settings) = &document.applets["system-monitor"].kind else {
            panic!("system-monitor resolves to its own kind");
        };
        assert_eq!(settings.chips, vec![Chip::Cpu, Chip::Ram]);
    }

    #[test]
    fn printing_defaults_and_refuses_an_unknown_key() {
        let default: crate::Config =
            toml::from_str("[applets.printing]\n").expect("the built-in printing applet loads");
        assert_eq!(
            default.applets["printing"].kind,
            Kind::Printing(Printing::default())
        );

        let configured: crate::Config = toml::from_str("[applets.printing]\njobs = 3\n")
            .expect("jobs is the printing applet's own setting");
        let Kind::Printing(settings) = &configured.applets["printing"].kind else {
            panic!("the table names the printing applet");
        };
        assert_eq!(settings.jobs, 3);

        toml::from_str::<crate::Config>("[applets.printing]\nbogus = 1\n")
            .expect_err("a struct variant refuses keys that are not its own");
    }

    #[test]
    fn privacy_defaults_and_refuses_an_unknown_key() {
        let default: crate::Config =
            toml::from_str("[applets.privacy]\n").expect("the built-in privacy applet loads");
        assert_eq!(
            default.applets["privacy"].kind,
            Kind::Privacy(Privacy::default())
        );

        let configured: crate::Config =
            toml::from_str("[applets.privacy]\nshow-location = false\n")
                .expect("show-location is the privacy applet's own setting");
        let Kind::Privacy(settings) = &configured.applets["privacy"].kind else {
            panic!("the table names the privacy applet");
        };
        assert!(!settings.show_location);

        toml::from_str::<crate::Config>("[applets.privacy]\nbogus = 1\n")
            .expect_err("a struct variant refuses keys that are not its own");
    }
}
