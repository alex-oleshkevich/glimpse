use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Panel {
    /// The panel's thickness in pixels: its height on a top or bottom edge, its width on a side.
    pub size: u32,
    /// The connector name of the one monitor this panel appears on, e.g. `eDP-1`. Unset puts a
    /// copy of the panel on every monitor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    /// The screen edge the panel sits on.
    pub position: Position,
    /// Space in pixels between the panel and the screen edges.
    pub margin: Margin,
    /// Applet names for the start of the panel: the left end, or the top on a side panel.
    pub left: Vec<String>,
    /// Applet names for the middle of the panel.
    pub center: Vec<String>,
    /// Applet names for the end of the panel: the right end, or the bottom on a side panel.
    pub right: Vec<String>,
    /// Whether the panel is shown when it starts. `glimpsectl panel show`, `hide` and `toggle`
    /// change it until the panel restarts or the panel configuration changes.
    pub visibility: Visibility,
    /// Slide a hidden panel in while the pointer rests on its screen edge.
    pub edge_reveal: bool,
    /// Milliseconds a revealed panel stays after the pointer leaves it, its popover closes or
    /// `glimpsectl panel conceal` runs.
    #[serde(deserialize_with = "hide_delay_ms")]
    #[schemars(range(max = 60000))]
    pub hide_delay_ms: u32,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            size: 36,
            monitor: None,
            position: Position::Top,
            margin: Margin::default(),
            left: names(&["workspace-name", "pager", "places", "mpris"]),
            center: names(&["clock", "weather", "notifications", "privacy"]),
            right: names(&[
                "next-event",
                "tray",
                "removable",
                "clipboard",
                "printing",
                "bluetooth",
                "network",
                "display",
                "audio",
                "idle",
                "power",
                "keyboard",
                "session",
            ]),
            visibility: Visibility::Show,
            edge_reveal: false,
            hide_delay_ms: 1000,
        }
    }
}

fn names(applets: &[&str]) -> Vec<String> {
    applets.iter().map(|name| (*name).to_owned()).collect()
}

fn hide_delay_ms<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    (0..=60_000)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 0 and 60000 milliseconds"))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    /// Shown, with its space reserved so windows sit beside it.
    #[default]
    Show,
    /// Hidden until `glimpsectl panel show`, `reveal` or, with `edge-reveal`, the pointer.
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Position {
    Left,
    Top,
    Right,
    Bottom,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Margin {
    /// Pixels between the panel and the left screen edge.
    pub left: u32,
    /// Pixels between the panel and the right screen edge.
    pub right: u32,
    /// Pixels between the panel and the top screen edge.
    pub top: u32,
    /// Pixels between the panel and the bottom screen edge.
    pub bottom: u32,
}
