use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// One applet on a bar: the settings every applet understands, and the ones its own kind does.
/// The table name is the applet's name, and `extends` says which kind it is when the two differ —
/// which is how one kind can appear more than once, as `[applets.clock-utc]` with
/// `extends = "clock"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Applet {
    #[serde(flatten)]
    pub common: Common,
    #[serde(flatten)]
    pub kind: Kind,
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
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
    /// A counter that ticks once a second. A development fixture: it proves the daemon is
    /// reachable and events are arriving, and is not meant for a real bar.
    Heartbeat {},
    /// Idle inhibition, for keeping the screen awake.
    Idle {},
    /// The active keyboard layout, and switches between the configured ones.
    Keyboard {},
    /// The currently playing track, with transport controls in its popover.
    Mpris {},
    /// Connection state, with the available networks in its popover.
    Network {},
    /// The next entry from the configured calendars.
    NextEvent {},
    /// Unread notifications, with their history in its popover.
    Notifications {},
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
    Weather {},
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
    /// How a time of day reads wherever the applet composes one itself — the world clock rows and
    /// the times inside event rows. `label-format` and `tooltip-format` are yours and are not
    /// affected by it.
    pub hour_format: HourFormat,
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

/// How a time of day reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HourFormat {
    /// Follow `LC_TIME`, which is what the locale's own `%X` resolves to.
    #[default]
    Locale,
    /// `3:30 PM`.
    #[serde(rename = "12h")]
    Twelve,
    /// `15:30`.
    #[serde(rename = "24h")]
    TwentyFour,
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

impl Default for Clock {
    fn default() -> Self {
        Self {
            label_format: "%H:%M".to_owned(),
            timezone: None,
            hour_format: HourFormat::default(),
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
    Ok(Applet {
        common,
        kind: Kind::deserialize(table).map_err(|error| name_the_common_settings(error, &keys))?,
    })
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
    use super::{COMMON, Common};

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
}
