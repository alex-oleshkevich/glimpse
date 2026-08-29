use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Panel {
    pub size: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    pub position: Position,
    pub margin: Margin,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            size: 36,
            monitor: None,
            position: Position::Top,
            margin: Margin::default(),
            left: names(&["pager", "mpris"]),
            center: names(&["clock", "weather", "notifications", "privacy"]),
            right: names(&[
                "next-event",
                "tray",
                "removable",
                "clipboard",
                "keyboard",
                "printing",
                "bluetooth",
                "network",
                "display",
                "audio",
                "idle",
                "battery",
                "session",
            ]),
        }
    }
}

fn names(applets: &[&str]) -> Vec<String> {
    applets.iter().map(|name| (*name).to_owned()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Position {
    Left,
    Top,
    Right,
    Bottom,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Margin {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}
