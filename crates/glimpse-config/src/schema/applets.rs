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
    /// `{index}` and `{name}` for the pager — the same way `label` already differs between them.
    /// Unset means the applet shows no tooltip.
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
    Battery {},
    /// Adapter state and paired devices.
    Bluetooth {},
    /// Display backlight level.
    Brightness {},
    /// Clipboard history.
    Clipboard {},
    /// The time and date, with a calendar in its popover.
    Clock(Clock),
    /// Runs a command and renders its output on the bar.
    Command {},
    /// Connected outputs, their modes and their arrangement.
    Display {},
    /// Hosts a third-party applet binary that draws its own popover.
    Exec {},
    /// A counter that ticks once a second. A development fixture: it proves the service is
    /// reachable and events are arriving, and is not meant for a real bar.
    Heartbeat {},
    /// Idle inhibition, for keeping the screen awake.
    Idle {},
    /// The active keyboard layout, and switches between the configured ones.
    Keyboard {},
    /// The currently playing track, with transport controls in its popover.
    Mpris(Mpris),
    /// Connection state, with the available networks in its popover.
    Network {},
    /// The next entry from the configured calendars.
    NextEvent(NextEvent),
    /// Unread notifications, with their history in its popover.
    Notifications(Notifications),
    /// A strip of workspaces or windows, one slot each, that switches between them on a click.
    Pager(Pager),
    /// Active print jobs.
    Printing {},
    /// Shows when the microphone, camera or screen is in use.
    Privacy {},
    /// Mounted removable drives, and unmounts them.
    Removable {},
    /// Log out, suspend, restart and shut down.
    Session {},
    /// The system tray: icons from applications that ask for one.
    Tray {},
    /// Current conditions, with the forecast in its popover.
    Weather(Weather),
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub timezones: Vec<Timezone>,
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
            label_format: "%H:%M".to_owned(),
            timezone: None,
            first_day: FirstDay::default(),
            week_numbers: true,
            timezones: Vec::new(),
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
    let kind = Kind::deserialize(table).map_err(|error| name_the_common_settings(error, &keys))?;
    on_earth(&kind)?;
    Ok(Applet {
        common,
        kind,
        regional: super::Regional::default(),
    })
}

/// The wire refuses these too, but a document saying so at load names the table and the key rather
/// than failing a `WatchPlace` call nobody is making.
fn on_earth(kind: &Kind) -> Result<(), toml::de::Error> {
    let Kind::Weather(weather) = kind else {
        return Ok(());
    };
    match &weather.place {
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
            let Some((city, country_code)) = name.split_once(',') else {
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
        }
        Place::Here {} => {}
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
    if common
        .settings_command
        .first()
        .is_some_and(|program| program.trim().is_empty())
    {
        return Err(toml::de::Error::custom(
            "settings-command names no program: its first element is what runs, and the rest are \
             that program's arguments",
        ));
    }
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
    use super::{COMMON, Common, Kind, NotificationIndicatorStyle};

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
}
