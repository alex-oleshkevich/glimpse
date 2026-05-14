use serde::Serialize;

use crate::protocol::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Fill,
    Start,
    End,
    Center,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Normal,
    Muted,
    Accent,
    Success,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Compact,
    #[default]
    Flat,
    Danger,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PagerAppearance {
    #[default]
    Dots,
    Numbers,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentFit {
    Fill,
    #[default]
    Contain,
    Cover,
    ScaleDown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LevelBarMode {
    #[default]
    Continuous,
    Discrete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct CommonProps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hexpand: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vexpand: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub halign: Option<Align>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valign: Option<Align>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
}

macro_rules! with_common {
    ($name:ident) => {
        impl $name {
            pub fn id(mut self, id: impl Into<String>) -> Self {
                self.common.id = Some(id.into());
                self
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Label {
    #[serde(flatten)]
    pub common: CommonProps,
    pub text: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub wrap: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xalign: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub selectable: bool,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            text: text.into(),
            wrap: false,
            xalign: None,
            selectable: false,
        }
    }
}

with_common!(Label);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Image {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: Icon,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_size: Option<i32>,
}

impl Image {
    pub fn new(icon: Icon) -> Self {
        Self {
            common: CommonProps::default(),
            icon,
            pixel_size: None,
        }
    }
}

with_common!(Image);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Picture {
    #[serde(flatten)]
    pub common: CommonProps,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_fit: Option<ContentFit>,
}

impl Picture {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            path: path.into(),
            content_fit: None,
        }
    }

    pub fn content_fit(mut self, content_fit: ContentFit) -> Self {
        self.content_fit = Some(content_fit);
        self
    }
}

with_common!(Picture);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Button {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<ButtonVariant>,
}

impl Button {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            label: None,
            icon: None,
            enabled: None,
            variant: None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }
}

with_common!(Button);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LinkButton {
    #[serde(flatten)]
    pub common: CommonProps,
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl LinkButton {
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            uri: uri.into(),
            label: None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

with_common!(LinkButton);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Expander {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
    pub expanded: bool,
    pub child: Box<TreeNode>,
}

impl Expander {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
            expanded: false,
            child: Box::new(Label::new("").into()),
        }
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn child(mut self, child: impl Into<TreeNode>) -> Self {
        self.child = Box::new(child.into());
        self
    }
}

with_common!(Expander);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeExpander {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode>,
    pub hide_expander: bool,
    pub indent_for_depth: bool,
    pub indent_for_icon: bool,
}

impl TreeExpander {
    pub fn new(child: impl Into<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            child: Box::new(child.into()),
            hide_expander: false,
            indent_for_depth: false,
            indent_for_icon: false,
        }
    }

    pub fn hide_expander(mut self, hide_expander: bool) -> Self {
        self.hide_expander = hide_expander;
        self
    }

    pub fn indent_for_depth(mut self, indent_for_depth: bool) -> Self {
        self.indent_for_depth = indent_for_depth;
        self
    }

    pub fn indent_for_icon(mut self, indent_for_icon: bool) -> Self {
        self.indent_for_icon = indent_for_icon;
        self
    }
}

with_common!(TreeExpander);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MenuButton {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub popover: Box<TreeNode>,
}

impl MenuButton {
    pub fn new(popover: impl Into<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            label: None,
            icon: None,
            popover: Box::new(popover.into()),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

with_common!(MenuButton);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Switch {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
}

impl Switch {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            label: None,
            active: false,
        }
    }
}

with_common!(Switch);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToggleButton {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
}

impl ToggleButton {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            label: None,
            active: false,
        }
    }
}

with_common!(ToggleButton);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Slider {
    #[serde(flatten)]
    pub common: CommonProps,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub draw_value: bool,
}

impl Slider {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            min: 0.0,
            max: 1.0,
            step: 0.1,
            value: 0.0,
            orientation: None,
            draw_value: false,
        }
    }
}

with_common!(Slider);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Checkbox {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
}

impl Checkbox {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            label: None,
            active: false,
        }
    }
}

with_common!(Checkbox);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectOption {
    pub id: String,
    pub label: String,
}

impl SelectOption {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Select {
    #[serde(flatten)]
    pub common: CommonProps,
    pub items: Vec<SelectOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<u32>,
}

impl Select {
    pub fn new(id: impl Into<String>, items: Vec<SelectOption>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            items,
            selected: None,
        }
    }
}

with_common!(Select);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Separator {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,
}

impl Separator {
    pub fn new() -> Self {
        Self {
            common: CommonProps::default(),
            orientation: None,
        }
    }
}

with_common!(Separator);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scroll {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode>,
}

impl Scroll {
    pub fn new(child: TreeNode) -> Self {
        Self {
            common: CommonProps::default(),
            child: Box::new(child),
        }
    }
}

with_common!(Scroll);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Overlay {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode>,
    pub overlays: Vec<TreeNode>,
}

impl Overlay {
    pub fn new(child: impl Into<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            child: Box::new(child.into()),
            overlays: Vec::new(),
        }
    }

    pub fn overlay(mut self, overlay: impl Into<TreeNode>) -> Self {
        self.overlays.push(overlay.into());
        self
    }

    pub fn overlays(mut self, overlays: Vec<TreeNode>) -> Self {
        self.overlays = overlays;
        self
    }
}

with_common!(Overlay);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListBox {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<TreeNode>,
}

impl ListBox {
    pub fn new(children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
        }
    }
}

with_common!(ListBox);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LevelBar {
    #[serde(flatten)]
    pub common: CommonProps,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub mode: LevelBarMode,
}

impl LevelBar {
    pub fn new(value: f64) -> Self {
        Self {
            common: CommonProps::default(),
            value,
            min: 0.0,
            max: 1.0,
            mode: LevelBarMode::Continuous,
        }
    }

    pub fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    pub fn mode(mut self, mode: LevelBarMode) -> Self {
        self.mode = mode;
        self
    }
}

with_common!(LevelBar);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GridChild {
    pub row: i32,
    pub column: i32,
    pub width: i32,
    pub height: i32,
    pub child: TreeNode,
}

impl GridChild {
    pub fn new(row: i32, column: i32, child: TreeNode) -> Self {
        Self {
            row,
            column,
            width: 1,
            height: 1,
            child,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Grid {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<GridChild>,
    pub row_spacing: i32,
    pub column_spacing: i32,
}

impl Grid {
    pub fn new(children: Vec<GridChild>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
            row_spacing: 0,
            column_spacing: 0,
        }
    }
}

with_common!(Grid);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoxNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub orientation: Orientation,
    pub spacing: i32,
    pub children: Vec<TreeNode>,
}

impl BoxNode {
    pub fn vertical(children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            orientation: Orientation::Vertical,
            spacing: 0,
            children,
        }
    }

    pub fn horizontal(children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            orientation: Orientation::Horizontal,
            spacing: 0,
            children,
        }
    }

    pub fn spacing(mut self, spacing: i32) -> Self {
        self.spacing = spacing;
        self
    }
}

with_common!(BoxNode);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hero {
    pub title: String,
    pub subtitle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(flatten)]
    pub common: CommonProps,
}

impl Hero {
    pub fn new(title: impl Into<String>, subtitle: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            icon: None,
            common: CommonProps::default(),
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

with_common!(Hero);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Progress {
    #[serde(flatten)]
    pub common: CommonProps,
    pub value: f64,
    pub max: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub show_text: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Progress {
    pub fn new(value: f64) -> Self {
        Self {
            common: CommonProps::default(),
            value,
            max: 1.0,
            show_text: false,
            text: None,
        }
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    pub fn show_text(mut self, show_text: bool) -> Self {
        self.show_text = show_text;
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

with_common!(Progress);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Card {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<TreeNode>,
}

impl Card {
    pub fn new(children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
        }
    }
}

with_common!(Card);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Section {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
    pub children: Vec<TreeNode>,
}

impl Section {
    pub fn new(title: impl Into<String>, children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            title: title.into(),
            subtitle: String::new(),
            children,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }
}

with_common!(Section);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Meter {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    pub label: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub interactive: bool,
}

impl Meter {
    pub fn new(label: impl Into<String>, value: f64, max: f64) -> Self {
        Self {
            common: CommonProps::default(),
            icon: None,
            label: label.into(),
            value,
            min: 0.0,
            max,
            step: 0.01,
            text: None,
            interactive: false,
        }
    }
}

with_common!(Meter);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Copyable {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub value: String,
}

impl Copyable {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
            value: value.into(),
        }
    }
}

with_common!(Copyable);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    #[serde(flatten)]
    pub common: CommonProps,
    pub spacing: i32,
    pub children: Vec<TreeNode>,
}

impl Row {
    pub fn new(children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            spacing: 0,
            children,
        }
    }

    pub fn spacing(mut self, spacing: i32) -> Self {
        self.spacing = spacing;
        self
    }
}

with_common!(Row);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Column {
    #[serde(flatten)]
    pub common: CommonProps,
    pub spacing: i32,
    pub children: Vec<TreeNode>,
}

impl Column {
    pub fn new(children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            spacing: 0,
            children,
        }
    }

    pub fn spacing(mut self, spacing: i32) -> Self {
        self.spacing = spacing;
        self
    }
}

with_common!(Column);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spinner {
    #[serde(flatten)]
    pub common: CommonProps,
    pub spinning: bool,
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            common: CommonProps::default(),
            spinning: true,
        }
    }

    pub fn spinning(mut self, spinning: bool) -> Self {
        self.spinning = spinning;
        self
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

with_common!(Spinner);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PropertyListItem {
    key: String,
    value: String,
}

impl PropertyListItem {
    fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PropertyList {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    rows: Vec<PropertyListItem>,
}

impl PropertyList {
    pub fn new<I, K, V>(rows: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            common: CommonProps::default(),
            title: String::new(),
            rows: rows
                .into_iter()
                .map(|(key, value)| PropertyListItem::new(key, value))
                .collect(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
}

impl Default for PropertyList {
    fn default() -> Self {
        Self::new(Vec::<(String, String)>::new())
    }
}

with_common!(PropertyList);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sublabel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<std::boxed::Box<TreeNode>>,
}

impl Item {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            icon: String::new(),
            label: label.into(),
            sublabel: String::new(),
            right: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn sublabel(mut self, sublabel: impl Into<String>) -> Self {
        self.sublabel = sublabel.into();
        self
    }

    pub fn right(mut self, right: impl Into<TreeNode>) -> Self {
        self.right = Some(std::boxed::Box::new(right.into()));
        self
    }
}

with_common!(Item);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionItem {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sublabel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<std::boxed::Box<TreeNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl ActionItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            icon: String::new(),
            label: label.into(),
            sublabel: String::new(),
            right: None,
            enabled: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn sublabel(mut self, sublabel: impl Into<String>) -> Self {
        self.sublabel = sublabel.into();
        self
    }

    pub fn right(mut self, right: impl Into<TreeNode>) -> Self {
        self.right = Some(std::boxed::Box::new(right.into()));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }
}

with_common!(ActionItem);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmptyState {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    pub subtitle: String,
}

impl EmptyState {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            title: title.into(),
            subtitle: String::new(),
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }
}

with_common!(EmptyState);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Badge {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
}

impl Badge {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
        }
    }
}

with_common!(Badge);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusDot {
    #[serde(flatten)]
    pub common: CommonProps,
}

impl StatusDot {
    pub fn new() -> Self {
        Self {
            common: CommonProps::default(),
        }
    }
}

with_common!(StatusDot);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PagerItem {
    #[serde(flatten)]
    pub common: CommonProps,
    pub appearance: PagerAppearance,
    pub label: String,
    pub active: bool,
    pub inactive: bool,
    pub occupied: bool,
    pub urgent: bool,
}

impl PagerItem {
    pub fn dots() -> Self {
        Self {
            common: CommonProps::default(),
            appearance: PagerAppearance::Dots,
            label: String::new(),
            active: false,
            inactive: false,
            occupied: false,
            urgent: false,
        }
    }

    pub fn number(label: impl Into<String>) -> Self {
        Self {
            appearance: PagerAppearance::Numbers,
            label: label.into(),
            ..Self::dots()
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn inactive(mut self, inactive: bool) -> Self {
        self.inactive = inactive;
        self
    }

    pub fn occupied(mut self, occupied: bool) -> Self {
        self.occupied = occupied;
        self
    }

    pub fn urgent(mut self, urgent: bool) -> Self {
        self.urgent = urgent;
        self
    }
}

impl Default for PagerItem {
    fn default() -> Self {
        Self::dots()
    }
}

with_common!(PagerItem);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PagerStrip {
    #[serde(flatten)]
    pub common: CommonProps,
    pub items: Vec<PagerItem>,
}

impl PagerStrip {
    pub fn new(items: Vec<PagerItem>) -> Self {
        Self {
            common: CommonProps::default(),
            items,
        }
    }
}

with_common!(PagerStrip);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TreeNode {
    Hero(Hero),
    Card(Card),
    Section(Section),
    Meter(Meter),
    Copyable(Copyable),
    PropertyList(PropertyList),
    Item(Item),
    ActionItem(ActionItem),
    EmptyState(EmptyState),
    Badge(Badge),
    #[serde(rename = "status")]
    StatusDot(StatusDot),
    PagerItem(PagerItem),
    PagerStrip(PagerStrip),
    Box(BoxNode),
    Row(Row),
    Column(Column),
    Grid(Grid),
    Scroll(Scroll),
    Overlay(Overlay),
    ListBox(ListBox),
    LevelBar(LevelBar),
    Progress(Progress),
    Separator(Separator),
    Spinner(Spinner),
    Label(Label),
    Image(Image),
    Picture(Picture),
    Button(Button),
    LinkButton(LinkButton),
    Expander(Expander),
    TreeExpander(TreeExpander),
    MenuButton(MenuButton),
    Switch(Switch),
    ToggleButton(ToggleButton),
    Slider(Slider),
    Select(Select),
    Checkbox(Checkbox),
}

impl From<Hero> for TreeNode {
    fn from(value: Hero) -> Self {
        Self::Hero(value)
    }
}
impl From<Card> for TreeNode {
    fn from(value: Card) -> Self {
        Self::Card(value)
    }
}
impl From<Section> for TreeNode {
    fn from(value: Section) -> Self {
        Self::Section(value)
    }
}
impl From<Meter> for TreeNode {
    fn from(value: Meter) -> Self {
        Self::Meter(value)
    }
}
impl From<Copyable> for TreeNode {
    fn from(value: Copyable) -> Self {
        Self::Copyable(value)
    }
}
impl From<Row> for TreeNode {
    fn from(value: Row) -> Self {
        Self::Row(value)
    }
}
impl From<Column> for TreeNode {
    fn from(value: Column) -> Self {
        Self::Column(value)
    }
}
impl From<Spinner> for TreeNode {
    fn from(value: Spinner) -> Self {
        Self::Spinner(value)
    }
}
impl From<PropertyList> for TreeNode {
    fn from(value: PropertyList) -> Self {
        Self::PropertyList(value)
    }
}
impl From<Item> for TreeNode {
    fn from(value: Item) -> Self {
        Self::Item(value)
    }
}
impl From<ActionItem> for TreeNode {
    fn from(value: ActionItem) -> Self {
        Self::ActionItem(value)
    }
}
impl From<EmptyState> for TreeNode {
    fn from(value: EmptyState) -> Self {
        Self::EmptyState(value)
    }
}
impl From<Badge> for TreeNode {
    fn from(value: Badge) -> Self {
        Self::Badge(value)
    }
}
impl From<StatusDot> for TreeNode {
    fn from(value: StatusDot) -> Self {
        Self::StatusDot(value)
    }
}
impl From<PagerItem> for TreeNode {
    fn from(value: PagerItem) -> Self {
        Self::PagerItem(value)
    }
}
impl From<PagerStrip> for TreeNode {
    fn from(value: PagerStrip) -> Self {
        Self::PagerStrip(value)
    }
}
impl From<BoxNode> for TreeNode {
    fn from(value: BoxNode) -> Self {
        Self::Box(value)
    }
}
impl From<Grid> for TreeNode {
    fn from(value: Grid) -> Self {
        Self::Grid(value)
    }
}
impl From<Scroll> for TreeNode {
    fn from(value: Scroll) -> Self {
        Self::Scroll(value)
    }
}
impl From<Overlay> for TreeNode {
    fn from(value: Overlay) -> Self {
        Self::Overlay(value)
    }
}
impl From<ListBox> for TreeNode {
    fn from(value: ListBox) -> Self {
        Self::ListBox(value)
    }
}
impl From<LevelBar> for TreeNode {
    fn from(value: LevelBar) -> Self {
        Self::LevelBar(value)
    }
}
impl From<Progress> for TreeNode {
    fn from(value: Progress) -> Self {
        Self::Progress(value)
    }
}
impl From<Separator> for TreeNode {
    fn from(value: Separator) -> Self {
        Self::Separator(value)
    }
}
impl From<Label> for TreeNode {
    fn from(value: Label) -> Self {
        Self::Label(value)
    }
}
impl From<Image> for TreeNode {
    fn from(value: Image) -> Self {
        Self::Image(value)
    }
}
impl From<Picture> for TreeNode {
    fn from(value: Picture) -> Self {
        Self::Picture(value)
    }
}
impl From<Button> for TreeNode {
    fn from(value: Button) -> Self {
        Self::Button(value)
    }
}
impl From<LinkButton> for TreeNode {
    fn from(value: LinkButton) -> Self {
        Self::LinkButton(value)
    }
}
impl From<Expander> for TreeNode {
    fn from(value: Expander) -> Self {
        Self::Expander(value)
    }
}
impl From<TreeExpander> for TreeNode {
    fn from(value: TreeExpander) -> Self {
        Self::TreeExpander(value)
    }
}
impl From<MenuButton> for TreeNode {
    fn from(value: MenuButton) -> Self {
        Self::MenuButton(value)
    }
}
impl From<Switch> for TreeNode {
    fn from(value: Switch) -> Self {
        Self::Switch(value)
    }
}
impl From<ToggleButton> for TreeNode {
    fn from(value: ToggleButton) -> Self {
        Self::ToggleButton(value)
    }
}
impl From<Slider> for TreeNode {
    fn from(value: Slider) -> Self {
        Self::Slider(value)
    }
}
impl From<Select> for TreeNode {
    fn from(value: Select) -> Self {
        Self::Select(value)
    }
}
impl From<Checkbox> for TreeNode {
    fn from(value: Checkbox) -> Self {
        Self::Checkbox(value)
    }
}
