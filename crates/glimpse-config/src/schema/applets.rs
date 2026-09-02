use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(
    tag = "extends",
    rename_all = "kebab-case",
    rename_all_fields = "kebab-case",
    deny_unknown_fields
)]
pub enum Applet {
    Audio {},
    Battery {},
    Bluetooth {},
    Brightness {},
    Clipboard {},
    Clock(Clock),
    Command {},
    Display {},
    Exec {},
    Heartbeat {},
    Idle {},
    Keyboard {},
    Mpris {},
    Network {},
    NextEvent {},
    Notifications {},
    Pager(Pager),
    Printing {},
    Privacy {},
    Removable {},
    Session {},
    Tray {},
    Weather {},
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Clock {
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Pager {
    pub mode: PagerMode,
    pub shape: PagerShape,
    pub scope: PagerScope,
    pub label: String,
    pub focused_label: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PagerMode {
    #[default]
    Workspaces,
    Windows,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PagerShape {
    #[default]
    Dots,
    Numbers,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
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
            focused_label: "{index}".to_owned(),
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
