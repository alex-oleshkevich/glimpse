use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StatusItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StatusPayload {
    #[serde(default)]
    pub items: Vec<StatusItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PopoverPayload {
    #[serde(default)]
    pub root: Option<TreeNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChildCommand {
    Status(StatusPayload),
    Popover(PopoverPayload),
    Class(String),
    ClosePopover,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum PanelCommand {
    Init(InitPayload),
    Event(EventPayload),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InitPayload {
    pub instance: String,
    pub options: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventPayload {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: EventKind,
    pub source: EventSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button: Option<MouseButton>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_y: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Click,
    Toggle,
    Change,
    Scroll,
    Open,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Status,
    Popover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Other,
}

impl MouseButton {
    pub fn from_number(button: u32) -> Self {
        match button {
            1 => Self::Left,
            2 => Self::Middle,
            3 => Self::Right,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CommonProps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub css_classes: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub styles: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceValue {
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S7,
    S8,
    S9,
    S10,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadiusValue {
    #[default]
    None,
    Sm,
    Md,
    Lg,
    Pill,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerBgValue {
    #[default]
    None,
    Surface,
    Raised,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontSizeValue {
    Xs,
    Sm,
    Base,
    Lg,
    Xl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontWeightValue {
    Normal,
    Medium,
    Semibold,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextColorValue {
    Normal,
    Muted,
    Accent,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeKindValue {
    #[default]
    Default,
    Success,
    Warning,
    Error,
    Accent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusDotStatusValue {
    #[default]
    Neutral,
    Success,
    Warning,
    Error,
    Accent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PagerAppearanceValue {
    #[default]
    Dots,
    Numbers,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopoverSizeValue {
    Small,
    #[default]
    Medium,
    Large,
    Wide,
}

impl PopoverSizeValue {
    pub fn class_name(self) -> &'static str {
        match self {
            Self::Small => "popover-size-small",
            Self::Medium => "popover-size-medium",
            Self::Large => "popover-size-large",
            Self::Wide => "popover-size-wide",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<FontSizeValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<FontWeightValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<TextColorValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xalign: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeaderNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeroNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_size: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle_sensitive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separator: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing: Option<Box<TreeNode>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BadgeNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
    #[serde(default)]
    pub kind: BadgeKindValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusDotNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub status: StatusDotStatusValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelIndicatorNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub checked: bool,
    #[serde(default)]
    pub needs_attention: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Box<TreeNode>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmptyStateNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpinnerNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default = "default_true")]
    pub spinning: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeterNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: f64,
    #[serde(default)]
    pub min: f64,
    #[serde(default = "default_meter_max")]
    pub max: f64,
    #[serde(default = "default_meter_step")]
    pub step: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default)]
    pub interactive: bool,
}

fn default_meter_max() -> f64 {
    1.0
}

fn default_meter_step() -> f64 {
    0.01
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeparatorNode {
    #[serde(flatten)]
    pub common: CommonProps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrollNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildrenNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub children: Vec<TreeNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<SpaceValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding_x: Option<SpaceValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding_y: Option<SpaceValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<SpaceValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_x: Option<SpaceValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_y: Option<SpaceValue>,
    #[serde(default)]
    pub radius: RadiusValue,
    #[serde(default)]
    pub bg: ContainerBgValue,
    #[serde(default)]
    pub border_width: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<SpaceValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<SpaceValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopoverShellNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub size: PopoverSizeValue,
    #[serde(default)]
    pub children: Vec<TreeNode>,
    #[serde(default)]
    pub footer: Vec<TreeNode>,
    #[serde(default)]
    pub footer_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub primary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Box<TreeNode>>,
    #[serde(default)]
    pub activatable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentedTileNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub primary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Box<TreeNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<TreeNode>>,
    #[serde(default)]
    pub expanded: bool,
    #[serde(default)]
    pub activatable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchTileNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    pub primary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode>>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpanderTileNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub primary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<TreeNode>>,
    #[serde(default)]
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliderTileNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode>>,
    #[serde(default)]
    pub value: f64,
    #[serde(default)]
    pub min: f64,
    #[serde(default = "default_slider_max")]
    pub max: f64,
    #[serde(default = "default_slider_step")]
    pub step: f64,
    #[serde(default = "default_slider_page")]
    pub page: f64,
    #[serde(default)]
    pub digits: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap_step: Option<f64>,
}

fn default_slider_max() -> f64 {
    1.0
}

fn default_slider_step() -> f64 {
    0.01
}

fn default_slider_page() -> f64 {
    0.1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChoiceTileNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub primary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode>>,
    #[serde(default)]
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceListChoice {
    pub id: String,
    pub primary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChoiceListNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(default)]
    pub choices: Vec<ChoiceListChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValueRow {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyValueGridNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub rows: Vec<KeyValueRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PagerItemNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: u64,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub appearance: PagerAppearanceValue,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub inactive: bool,
    #[serde(default)]
    pub occupied: bool,
    #[serde(default)]
    pub urgent: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PagerStripNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub placeholder: bool,
    #[serde(default)]
    pub items: Vec<PagerItemNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveIndicatorNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenCastIndicatorNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub selected_date: String,
    #[serde(default)]
    pub event_days: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatteryHeroNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: String,
    pub percentage: String,
    pub fraction: f64,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateHeroNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub weekday: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventItemNode {
    pub id: String,
    pub title: String,
    pub start: String,
    #[serde(default)]
    pub end: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default)]
    pub all_day: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventsNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub date: String,
    #[serde(default)]
    pub events: Vec<EventItemNode>,
    #[serde(default)]
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeatherForecastItemNode {
    pub day_name: String,
    pub icon: String,
    pub condition: String,
    pub temperatures: String,
    #[serde(default)]
    pub is_today: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherForecastListNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub items: Vec<WeatherForecastItemNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeatherHourlyItemNode {
    pub time: String,
    pub icon: String,
    pub temperature: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherHourlyStripNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub items: Vec<WeatherHourlyItemNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldClockRowNode {
    pub name: String,
    pub timezone: String,
    pub time: String,
    pub offset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldClockNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub rows: Vec<WorldClockRowNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TreeNode {
    Row(ChildrenNode),
    Column(ChildrenNode),
    Container(ContainerNode),
    BoxedList(ChildrenNode),
    PopoverShell(PopoverShellNode),
    Text(TextNode),
    Header(HeaderNode),
    Hero(HeroNode),
    Badge(BadgeNode),
    StatusDot(StatusDotNode),
    PanelIndicator(PanelIndicatorNode),
    EmptyState(EmptyStateNode),
    Spinner(SpinnerNode),
    Meter(MeterNode),
    Separator(SeparatorNode),
    Scroll(ScrollNode),
    Tile(TileNode),
    SegmentedTile(SegmentedTileNode),
    ButtonRow(ChildrenNode),
    SwitchTile(SwitchTileNode),
    ExpanderTile(ExpanderTileNode),
    SliderTile(SliderTileNode),
    ChoiceTile(ChoiceTileNode),
    ChoiceList(ChoiceListNode),
    KeyValueGrid(KeyValueGridNode),
    PagerItem(PagerItemNode),
    PagerStrip(PagerStripNode),
    CameraIndicator(ActiveIndicatorNode),
    MicIndicator(ActiveIndicatorNode),
    MutedIndicator(ActiveIndicatorNode),
    #[serde(rename = "screencast_indicator")]
    ScreenCastIndicator(ScreenCastIndicatorNode),
    LocationIndicator(ActiveIndicatorNode),
    Calendar(CalendarNode),
    BatteryHero(BatteryHeroNode),
    DateHero(DateHeroNode),
    Events(EventsNode),
    WeatherForecastList(WeatherForecastListNode),
    WeatherHourlyStrip(WeatherHourlyStripNode),
    WorldClock(WorldClockNode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    EmptyLine,
    MissingPayload { command: String },
    UnknownCommand(String),
    InvalidJson { command: String, message: String },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLine => write!(f, "empty protocol line"),
            Self::MissingPayload { command } => {
                write!(f, "{command} command requires JSON payload")
            }
            Self::UnknownCommand(command) => write!(f, "unknown exec protocol command: {command}"),
            Self::InvalidJson { command, message } => {
                write!(f, "invalid JSON payload for {command}: {message}")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

pub fn parse_child_line(line: &str) -> Result<ChildCommand, ProtocolError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(ProtocolError::EmptyLine);
    }

    if let Some(class) = line.strip_prefix("class ") {
        return Ok(ChildCommand::Class(class.trim().to_string()));
    }

    if line == "close-popover" {
        return Ok(ChildCommand::ClosePopover);
    }

    let (command, payload) = split_line(line)?;
    match command {
        "status" => Ok(ChildCommand::Status(decode_payload(command, payload)?)),
        "popover" => Ok(ChildCommand::Popover(decode_payload(command, payload)?)),
        other => Err(ProtocolError::UnknownCommand(other.to_string())),
    }
}

pub fn encode_panel_command(command: &PanelCommand) -> String {
    match command {
        PanelCommand::Init(payload) => {
            format!(
                "init {}",
                serde_json::to_string(payload).expect("init payload should serialize")
            )
        }
        PanelCommand::Event(payload) => {
            format!(
                "event {}",
                serde_json::to_string(payload).expect("event payload should serialize")
            )
        }
    }
}

fn split_line(line: &str) -> Result<(&str, &str), ProtocolError> {
    let Some((command, payload)) = line.split_once(' ') else {
        return Err(ProtocolError::MissingPayload {
            command: line.to_string(),
        });
    };
    let payload = payload.trim();
    if payload.is_empty() {
        return Err(ProtocolError::MissingPayload {
            command: command.to_string(),
        });
    }
    Ok((command, payload))
}

fn decode_payload<'de, T>(command: &str, payload: &'de str) -> Result<T, ProtocolError>
where
    T: Deserialize<'de>,
{
    serde_json::from_str(payload).map_err(|error| ProtocolError::InvalidJson {
        command: command.into(),
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_line_with_object_icons_and_labels() {
        let command = parse_child_line(
            r#"status {"items":[{"id":"cpu","icon":"cpu-symbolic","label":"12%","tooltip":"CPU usage"}]}"#,
        )
        .expect("status line should parse");

        assert_eq!(
            command,
            ChildCommand::Status(StatusPayload {
                items: vec![StatusItem {
                    id: Some("cpu".into()),
                    icon: Some("cpu-symbolic".into()),
                    label: Some("12%".into()),
                    tooltip: Some("CPU usage".into()),
                }]
            })
        );
    }

    #[test]
    fn parses_shared_widget_popover_line() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"popover_shell","data":{"children":[{"type":"tile","data":{"id":"wifi","primary":"Wi-Fi","activatable":true}}]}}}"#,
        )
        .expect("popover line should parse");

        assert!(matches!(
            command,
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::PopoverShell(_))
            })
        ));
    }

    #[test]
    fn rejects_removed_legacy_widget_nodes() {
        let error = parse_child_line(
            r#"popover {"root":{"type":"button","data":{"id":"refresh","label":"Refresh"}}}"#,
        )
        .expect_err("legacy button nodes should be unsupported");

        assert!(error.to_string().contains("popover"));
    }

    #[test]
    fn parses_class_line_as_bare_name() {
        let command = parse_child_line("class workstation").expect("class line should parse");
        assert_eq!(command, ChildCommand::Class("workstation".into()));
    }

    #[test]
    fn encodes_init_and_event_lines() {
        assert_eq!(
            encode_panel_command(&PanelCommand::Init(InitPayload {
                instance: "sysinfo".into(),
                options: serde_json::json!({"interval": 5}),
            })),
            r#"init {"instance":"sysinfo","options":{"interval":5}}"#.to_string()
        );

        assert_eq!(
            encode_panel_command(&PanelCommand::Event(EventPayload {
                id: "refresh".into(),
                kind: EventKind::Click,
                source: EventSource::Popover,
                button: Some(MouseButton::Left),
                active: None,
                value: None,
                delta_y: None,
            })),
            r#"event {"id":"refresh","type":"click","source":"popover","button":"left"}"#
                .to_string()
        );
    }
}
