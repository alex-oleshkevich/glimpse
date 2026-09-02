use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// One applet on a bar. The table name is the applet's name, and `extends` says which kind it is
/// when the two differ — which is how one kind can appear more than once, as `[applets.clock-utc]`
/// with `extends = "clock"`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(
    tag = "extends",
    rename_all = "kebab-case",
    rename_all_fields = "kebab-case",
    deny_unknown_fields
)]
pub enum Applet {
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
    /// A `strftime` format string, such as `%H:%M` or `%a %d %b %H:%M`.
    pub format: String,
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
    /// What a slot reads, when the shape is `numbers`, and the fallback for every state below.
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
/// How a slot is drawn: `dots` for a dot each with the current one drawn longer, which takes the
/// least room on the bar, or `numbers` for the slot's label in a pill.
#[serde(rename_all = "kebab-case")]
pub enum PagerShape {
    #[default]
    Dots,
    Numbers,
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
            format: "%H:%M".to_owned(),
        }
    }
}

impl Applet {
    pub fn from_name(name: &str) -> Option<Self> {
        let mut table = toml::Table::new();
        table.insert("extends".to_owned(), toml::Value::String(name.to_owned()));
        Self::deserialize(table).ok()
    }
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<String, Applet>, D::Error>
where
    D: Deserializer<'de>,
{
    BTreeMap::<String, toml::Table>::deserialize(deserializer)?
        .into_iter()
        .map(|(name, mut table)| {
            table
                .entry("extends")
                .or_insert_with(|| toml::Value::String(name.clone()));
            Applet::deserialize(table)
                .map(|applet| (name.clone(), applet))
                .map_err(|error| D::Error::custom(format!("[applets.{name}]: {error}")))
        })
        .collect()
}

pub fn schema(generator: &mut SchemaGenerator) -> Schema {
    let aliased = Applet::json_schema(generator);
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
