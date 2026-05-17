use std::fmt;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CommonProps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hexpand: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vexpand: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halign: Option<AlignValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valign: Option<AlignValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub css_classes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Normal,
    Muted,
    Accent,
    Success,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Compact,
    #[default]
    Flat,
    Danger,
}

impl Variant {
    pub fn class_name(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::Muted => Some("is-muted"),
            Self::Accent => Some("is-accent"),
            Self::Success => Some("is-success"),
            Self::Warning => Some("is-warning"),
            Self::Danger => Some("is-danger"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignValue {
    Fill,
    Start,
    End,
    Center,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrientationValue {
    Horizontal,
    Vertical,
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
    pub switch: Option<bool>,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub spacing: i32,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<TreeNode>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<TreeNode>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyListItem {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyListNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub rows: Vec<PropertyListItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub left: Option<Box<TreeNode>>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub sublabel: String,
    #[serde(default)]
    pub right: Option<Box<TreeNode>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionItemNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(default)]
    pub left: Option<Box<TreeNode>>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub sublabel: String,
    #[serde(default)]
    pub right: Option<Box<TreeNode>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmptyStateNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BadgeNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PagerAppearanceValue {
    #[default]
    Dots,
    Numbers,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentFitValue {
    Fill,
    #[default]
    Contain,
    Cover,
    ScaleDown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LevelBarModeValue {
    #[default]
    Continuous,
    Discrete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PagerItemNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub appearance: PagerAppearanceValue,
    #[serde(default)]
    pub label: String,
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
    pub items: Vec<PagerItemNode>,
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
    pub value: f64,
    #[serde(default)]
    pub min: f64,
    #[serde(default = "default_progress_max")]
    pub max: f64,
    #[serde(default = "default_meter_step")]
    pub step: f64,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub interactive: bool,
}

fn default_meter_step() -> f64 {
    0.01
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopyableNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub label: String,
    pub value: String,
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
pub struct LabelNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub text: String,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub xalign: Option<f32>,
    #[serde(default)]
    pub selectable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IconNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: String,
    #[serde(default)]
    pub pixel_size: Option<i32>,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PictureNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub path: String,
    #[serde(default)]
    pub content_fit: ContentFitValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ButtonNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub variant: ButtonVariant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkButtonNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub uri: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpanderNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
    #[serde(default)]
    pub expanded: bool,
    pub child: Box<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TreeExpanderNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode>,
    #[serde(default)]
    pub hide_expander: bool,
    #[serde(default)]
    pub indent_for_depth: bool,
    #[serde(default)]
    pub indent_for_icon: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuButtonNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    pub popover: Box<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToggleButtonNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckboxNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliderNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub value: f64,
    #[serde(default)]
    pub orientation: Option<OrientationValue>,
    #[serde(default)]
    pub draw_value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(default)]
    pub items: Vec<SelectOption>,
    #[serde(default)]
    pub selected: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub value: f64,
    #[serde(default = "default_progress_max")]
    pub max: f64,
    #[serde(default)]
    pub show_text: bool,
    #[serde(default)]
    pub text: Option<String>,
}

fn default_progress_max() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelBarNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub value: f64,
    #[serde(default)]
    pub min: f64,
    #[serde(default = "default_level_bar_max")]
    pub max: f64,
    #[serde(default)]
    pub mode: LevelBarModeValue,
}

fn default_level_bar_max() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeparatorNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub orientation: Option<OrientationValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrollNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode>,
    #[serde(default)]
    pub overlays: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListBoxNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridChildNode {
    pub row: i32,
    pub column: i32,
    #[serde(default = "default_grid_span")]
    pub width: i32,
    #[serde(default = "default_grid_span")]
    pub height: i32,
    pub child: TreeNode,
}

fn default_grid_span() -> i32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub row_spacing: i32,
    #[serde(default)]
    pub column_spacing: i32,
    #[serde(default)]
    pub children: Vec<GridChildNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TreeNode {
    Hero(HeroNode),
    Card(CardNode),
    Section(SectionNode),
    Meter(MeterNode),
    Copyable(CopyableNode),
    Column(LayoutNode),
    Row(LayoutNode),
    PropertyList(PropertyListNode),
    Item(ItemNode),
    ActionItem(ActionItemNode),
    EmptyState(EmptyStateNode),
    Badge(BadgeNode),
    Status(StatusNode),
    PagerItem(PagerItemNode),
    PagerStrip(PagerStripNode),
    Spinner(SpinnerNode),
    Grid(GridNode),
    Scroll(ScrollNode),
    Overlay(OverlayNode),
    ListBox(ListBoxNode),
    LevelBar(LevelBarNode),
    Progress(ProgressNode),
    Separator(SeparatorNode),
    Label(LabelNode),
    Icon(IconNode),
    Picture(PictureNode),
    Button(ButtonNode),
    LinkButton(LinkButtonNode),
    Expander(ExpanderNode),
    TreeExpander(TreeExpanderNode),
    MenuButton(MenuButtonNode),
    Switch(SwitchNode),
    ToggleButton(ToggleButtonNode),
    Checkbox(CheckboxNode),
    Slider(SliderNode),
    Select(SelectNode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    MissingCommand,
    MissingPayload { command: String },
    UnknownCommand { command: String },
    InvalidJson { command: String, message: String },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCommand => write!(f, "missing command"),
            Self::MissingPayload { command } => write!(f, "{command}: missing JSON payload"),
            Self::UnknownCommand { command } => write!(f, "unknown exec command {command}"),
            Self::InvalidJson { command, message } => {
                write!(f, "{command}: invalid JSON payload: {message}")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

pub fn parse_child_line(line: &str) -> Result<ChildCommand, ProtocolError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(ProtocolError::MissingCommand);
    }

    let (command, payload) = split_line(line)?;
    match command {
        "status" => decode_payload(command, payload).map(ChildCommand::Status),
        "popover" => decode_payload(command, payload).map(ChildCommand::Popover),
        "class" => Ok(ChildCommand::Class(payload.to_owned())),
        other => Err(ProtocolError::UnknownCommand {
            command: other.into(),
        }),
    }
}

pub fn encode_panel_command(command: &PanelCommand) -> String {
    let (name, payload) = match command {
        PanelCommand::Init(payload) => (
            "init",
            serde_json::to_string(payload).expect("init payload should serialize"),
        ),
        PanelCommand::Event(payload) => (
            "event",
            serde_json::to_string(payload).expect("event payload should serialize"),
        ),
    };
    format!("{name} {payload}")
}

fn split_line(line: &str) -> Result<(&str, &str), ProtocolError> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let command = parts.next().filter(|command| !command.is_empty());
    let payload = parts.next().map(str::trim_start);
    match (command, payload) {
        (Some(command), Some(payload)) if !payload.is_empty() => Ok((command, payload)),
        (Some(command), _) => Err(ProtocolError::MissingPayload {
            command: command.into(),
        }),
        _ => Err(ProtocolError::MissingCommand),
    }
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
    fn parses_popover_line_with_root_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"section","data":{"title":"System","children":[{"type":"button","data":{"id":"refresh","label":"Refresh"}}]}}}"#,
        )
        .expect("popover line should parse");

        assert!(matches!(
            command,
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::Section(_))
            })
        ));
    }

    #[test]
    fn parses_layer_two_nodes() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"section","data":{"title":"System","children":[{"type":"row","data":{"children":[{"type":"label","data":{"text":"Wi-Fi"}},{"type":"badge","data":{"label":"on"}}]}},{"type":"section","data":{"title":"Details","subtitle":"More","children":[{"type":"meter","data":{"id":"volume","label":"Volume","value":0.5,"interactive":true}},{"type":"copyable","data":{"label":"ID","value":"device-42"}},{"type":"button","data":{"id":"refresh","label":"Refresh","variant":"primary"}}]}}]}}}"#,
        )
        .expect("layer two nodes should parse");

        assert!(matches!(
            command,
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::Section(_))
            })
        ));
    }

    #[test]
    fn parses_item_node_with_left_and_right_slots() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"item","data":{"left":{"type":"icon","data":{"icon":"network-wireless-symbolic","pixel_size":16}},"label":"Wi-Fi","sublabel":"Connected","right":{"type":"badge","data":{"label":"home-5G"}}}}}"#,
        )
        .expect("item node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::Item(item)),
            }) => {
                assert!(matches!(item.left.as_deref(), Some(TreeNode::Icon(_))));
                assert_eq!(item.label, "Wi-Fi");
                assert_eq!(item.sublabel, "Connected");
                assert!(matches!(item.right.as_deref(), Some(TreeNode::Badge(_))));
            }
            other => panic!("expected item popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_action_item_node_with_left_and_right_slots() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"action_item","data":{"id":"wifi","left":{"type":"icon","data":{"icon":"network-wireless-symbolic","pixel_size":16}},"label":"Wi-Fi","sublabel":"Connected","right":{"type":"badge","data":{"label":"home-5G"}}}}}"#,
        )
        .expect("action item node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::ActionItem(item)),
            }) => {
                assert_eq!(item.id.as_str(), "wifi");
                assert!(matches!(item.left.as_deref(), Some(TreeNode::Icon(_))));
                assert_eq!(item.label, "Wi-Fi");
                assert_eq!(item.sublabel, "Connected");
                assert!(matches!(item.right.as_deref(), Some(TreeNode::Badge(_))));
            }
            other => panic!("expected action item popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_picture_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"picture","data":{"path":"/home/me/photo.png","content_fit":"cover"}}}"#,
        )
        .expect("picture node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::Picture(picture)),
            }) => {
                assert_eq!(picture.path, "/home/me/photo.png");
                assert_eq!(picture.content_fit, ContentFitValue::Cover);
            }
            other => panic!("expected picture popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_toggle_button_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"toggle_button","data":{"id":"wifi","label":"Wi-Fi","active":true}}}"#,
        )
        .expect("toggle button node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::ToggleButton(toggle)),
            }) => {
                assert_eq!(toggle.id.as_str(), "wifi");
                assert_eq!(toggle.label.as_deref(), Some("Wi-Fi"));
                assert!(toggle.active);
            }
            other => panic!("expected toggle button popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_link_button_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"link_button","data":{"uri":"https://example.com/docs","label":"Docs"}}}"#,
        )
        .expect("link button node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::LinkButton(link)),
            }) => {
                assert_eq!(link.uri, "https://example.com/docs");
                assert_eq!(link.label.as_deref(), Some("Docs"));
            }
            other => panic!("expected link button popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_expander_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"expander","data":{"label":"Details","expanded":true,"child":{"type":"label","data":{"text":"More"}}}}}"#,
        )
        .expect("expander node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::Expander(expander)),
            }) => {
                assert_eq!(expander.label, "Details");
                assert!(expander.expanded);
                assert!(matches!(expander.child.as_ref(), TreeNode::Label(_)));
            }
            other => panic!("expected expander popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_tree_expander_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"tree_expander","data":{"child":{"type":"label","data":{"text":"Nested"}},"hide_expander":true,"indent_for_depth":true,"indent_for_icon":true}}}"#,
        )
        .expect("tree expander node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::TreeExpander(tree_expander)),
            }) => {
                assert!(matches!(tree_expander.child.as_ref(), TreeNode::Label(_)));
                assert!(tree_expander.hide_expander);
                assert!(tree_expander.indent_for_depth);
                assert!(tree_expander.indent_for_icon);
            }
            other => panic!("expected tree expander popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_menu_button_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"menu_button","data":{"label":"More","icon":"open-menu-symbolic","popover":{"type":"label","data":{"text":"Menu content"}}}}}"#,
        )
        .expect("menu button node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::MenuButton(menu_button)),
            }) => {
                assert_eq!(menu_button.label.as_deref(), Some("More"));
                assert_eq!(menu_button.icon.as_deref(), Some("open-menu-symbolic"));
                assert!(matches!(menu_button.popover.as_ref(), TreeNode::Label(_)));
            }
            other => panic!("expected menu button popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_overlay_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"overlay","data":{"child":{"type":"label","data":{"text":"Base"}},"overlays":[{"type":"badge","data":{"label":"Top"}}]}}}"#,
        )
        .expect("overlay node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::Overlay(overlay)),
            }) => {
                assert!(matches!(overlay.child.as_ref(), TreeNode::Label(_)));
                assert_eq!(overlay.overlays.len(), 1);
                assert!(matches!(overlay.overlays.first(), Some(TreeNode::Badge(_))));
            }
            other => panic!("expected overlay popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_list_box_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"list_box","data":{"children":[{"type":"label","data":{"text":"First"}},{"type":"badge","data":{"label":"Second"}}]}}}"#,
        )
        .expect("list box node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::ListBox(list_box)),
            }) => {
                assert_eq!(list_box.children.len(), 2);
                assert!(matches!(
                    list_box.children.first(),
                    Some(TreeNode::Label(_))
                ));
                assert!(matches!(list_box.children.get(1), Some(TreeNode::Badge(_))));
            }
            other => panic!("expected list box popover, got {other:?}"),
        }
    }

    #[test]
    fn parses_level_bar_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"level_bar","data":{"value":0.7,"min":0.0,"max":1.0,"mode":"continuous"}}}"#,
        )
        .expect("level bar node should parse");

        match command {
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::LevelBar(level_bar)),
            }) => {
                assert_eq!(level_bar.value, 0.7);
                assert_eq!(level_bar.min, 0.0);
                assert_eq!(level_bar.max, 1.0);
                assert_eq!(level_bar.mode, LevelBarModeValue::Continuous);
            }
            other => panic!("expected level bar popover, got {other:?}"),
        }
    }

    #[test]
    fn rejects_text_entry_nodes() {
        let error = parse_child_line(
            r#"popover {"root":{"type":"entry","data":{"id":"name","text":"bad"}}}"#,
        )
        .expect_err("text entry nodes should be unsupported");

        assert!(error.to_string().contains("popover"));
    }

    #[test]
    fn rejects_collapsible_nodes() {
        let error = parse_child_line(
            r#"popover {"root":{"type":"collapsible","data":{"title":"Details","expanded":false,"body":[]}}}"#,
        )
        .expect_err("collapsible nodes should be unsupported");

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

        assert_eq!(
            encode_panel_command(&PanelCommand::Event(EventPayload {
                id: "popover".into(),
                kind: EventKind::Open,
                source: EventSource::Popover,
                button: None,
                active: None,
                value: None,
                delta_y: None,
            })),
            r#"event {"id":"popover","type":"open","source":"popover"}"#.to_string()
        );
    }
}
