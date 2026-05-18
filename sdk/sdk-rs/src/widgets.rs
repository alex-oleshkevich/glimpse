use std::collections::BTreeMap;

use serde::Serialize;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Space {
    None,
    Xxs,
    Xs,
    Sm,
    Md,
    Lg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    Bg,
    Fg,
    Surface,
    SurfaceRaised,
    Border,
    MutedFg,
    Accent,
    AccentFg,
    Success,
    SuccessFg,
    Warning,
    WarningFg,
    Danger,
    DangerFg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Radius {
    None,
    Sm,
    Md,
    Lg,
    Pill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FontSize {
    Xxs,
    Xs,
    Sm,
    Md,
    Base,
    Lg,
    Xl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FontWeight {
    Normal,
    Medium,
    Semibold,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BorderWidth {
    None,
    Thin,
    Medium,
    Thick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusVariant {
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub css_classes: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub styles: BTreeMap<String, String>,
}

macro_rules! with_common {
    ($name:ident) => {
        impl $name {
            pub fn css_class(mut self, class: impl Into<String>) -> Self {
                self.common.css_classes.push(class.into());
                self
            }

            pub fn style(mut self, property: impl Into<String>, value: impl Into<String>) -> Self {
                self.common.styles.insert(property.into(), value.into());
                self
            }

            pub fn styles(
                mut self,
                styles: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
            ) -> Self {
                self.common.styles.extend(
                    styles
                        .into_iter()
                        .map(|(property, value)| (property.into(), value.into())),
                );
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            text: text.into(),
            wrap: false,
            xalign: None,
            selectable: false,
            variant: None,
        }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = Some(variant);
        self
    }
}

with_common!(Label);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Icon {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_size: Option<i32>,
}

impl Icon {
    pub fn new(icon: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            icon: icon.into(),
            pixel_size: None,
        }
    }
}

with_common!(Icon);

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
    pub id: String,
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
            id: id.into(),
            common: CommonProps::default(),
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
pub struct Switch {
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
}

impl Switch {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
            label: None,
            active: false,
        }
    }
}

with_common!(Switch);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToggleButton {
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub active: bool,
}

impl ToggleButton {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
            label: None,
            icon: None,
            active: false,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

with_common!(ToggleButton);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Slider {
    pub id: String,
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
            id: id.into(),
            common: CommonProps::default(),
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
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
}

impl Checkbox {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
            label: None,
            active: false,
        }
    }
}

with_common!(Checkbox);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Select {
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(serialize_with = "serialize_select_items")]
    pub items: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<u32>,
}

fn serialize_select_items<S>(items: &[(String, String)], s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(Some(items.len()))?;
    for (id, label) in items {
        seq.serialize_element(&serde_json::json!({"id": id, "label": label}))?;
    }
    seq.end()
}

impl Select {
    pub fn new(id: impl Into<String>, items: Vec<(String, String)>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
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
            row_spacing: 4,
            column_spacing: 4,
        }
    }
}

with_common!(Grid);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hero {
    pub title: String,
    pub subtitle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch: Option<bool>,
    #[serde(flatten)]
    pub common: CommonProps,
}

impl Hero {
    pub fn new(title: impl Into<String>, subtitle: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            icon: None,
            id: None,
            switch: None,
            common: CommonProps::default(),
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn switch(mut self, active: bool) -> Self {
        self.switch = Some(active);
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<TreeNode>>,
}

impl Card {
    pub fn new(child: Option<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            child: child.map(Box::new),
        }
    }
}

with_common!(Card);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Container {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<TreeNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_top: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_right: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_bottom: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_left: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_top: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_right: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_bottom: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_left: Option<Space>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<Radius>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<BorderWidth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<FontSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<FontWeight>,
}

impl Container {
    pub fn new(child: Option<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            child: child.map(Box::new),
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            margin: None,
            margin_top: None,
            margin_right: None,
            margin_bottom: None,
            margin_left: None,
            padding: None,
            padding_top: None,
            padding_right: None,
            padding_bottom: None,
            padding_left: None,
            background: None,
            color: None,
            border_radius: None,
            border_width: None,
            border_color: None,
            font_size: None,
            font_weight: None,
        }
    }

    pub fn child(mut self, child: impl Into<TreeNode>) -> Self {
        self.child = Some(Box::new(child.into()));
        self
    }

    pub fn width(mut self, width: i32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: i32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn min_width(mut self, min_width: i32) -> Self {
        self.min_width = Some(min_width);
        self
    }

    pub fn min_height(mut self, min_height: i32) -> Self {
        self.min_height = Some(min_height);
        self
    }

    pub fn margin(mut self, margin: Space) -> Self {
        self.margin = Some(margin);
        self
    }

    pub fn margin_top(mut self, margin_top: Space) -> Self {
        self.margin_top = Some(margin_top);
        self
    }

    pub fn margin_right(mut self, margin_right: Space) -> Self {
        self.margin_right = Some(margin_right);
        self
    }

    pub fn margin_bottom(mut self, margin_bottom: Space) -> Self {
        self.margin_bottom = Some(margin_bottom);
        self
    }

    pub fn margin_left(mut self, margin_left: Space) -> Self {
        self.margin_left = Some(margin_left);
        self
    }

    pub fn padding(mut self, padding: Space) -> Self {
        self.padding = Some(padding);
        self
    }

    pub fn padding_top(mut self, padding_top: Space) -> Self {
        self.padding_top = Some(padding_top);
        self
    }

    pub fn padding_right(mut self, padding_right: Space) -> Self {
        self.padding_right = Some(padding_right);
        self
    }

    pub fn padding_bottom(mut self, padding_bottom: Space) -> Self {
        self.padding_bottom = Some(padding_bottom);
        self
    }

    pub fn padding_left(mut self, padding_left: Space) -> Self {
        self.padding_left = Some(padding_left);
        self
    }

    pub fn background(mut self, background: Color) -> Self {
        self.background = Some(background);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn border_radius(mut self, border_radius: Radius) -> Self {
        self.border_radius = Some(border_radius);
        self
    }

    pub fn border_width(mut self, border_width: BorderWidth) -> Self {
        self.border_width = Some(border_width);
        self
    }

    pub fn border_color(mut self, border_color: Color) -> Self {
        self.border_color = Some(border_color);
        self
    }

    pub fn font_size(mut self, font_size: FontSize) -> Self {
        self.font_size = Some(font_size);
        self
    }

    pub fn font_weight(mut self, font_weight: FontWeight) -> Self {
        self.font_weight = Some(font_weight);
        self
    }
}

with_common!(Container);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Meter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
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
            id: None,
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

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
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
            spacing: 4,
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
            spacing: 4,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<std::boxed::Box<TreeNode>>,
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
            left: None,
            label: label.into(),
            sublabel: String::new(),
            right: None,
        }
    }

    pub fn icon(mut self, name: impl Into<String>) -> Self {
        self.left = Some(std::boxed::Box::new(TreeNode::Icon(Icon {
            common: CommonProps::default(),
            icon: name.into(),
            pixel_size: Some(16),
        })));
        self
    }

    pub fn left(mut self, left: impl Into<TreeNode>) -> Self {
        self.left = Some(std::boxed::Box::new(left.into()));
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
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<std::boxed::Box<TreeNode>>,
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
            id: id.into(),
            common: CommonProps::default(),
            left: None,
            label: label.into(),
            sublabel: String::new(),
            right: None,
            enabled: None,
        }
    }

    pub fn icon(mut self, name: impl Into<String>) -> Self {
        self.left = Some(std::boxed::Box::new(TreeNode::Icon(Icon {
            common: CommonProps::default(),
            icon: name.into(),
            pixel_size: Some(16),
        })));
        self
    }

    pub fn left(mut self, left: impl Into<TreeNode>) -> Self {
        self.left = Some(std::boxed::Box::new(left.into()));
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
}

impl Badge {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
            variant: None,
        }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = Some(variant);
        self
    }
}

with_common!(Badge);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusDot {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<StatusVariant>,
}

impl StatusDot {
    pub fn new() -> Self {
        Self {
            common: CommonProps::default(),
            variant: None,
        }
    }

    pub fn variant(mut self, variant: StatusVariant) -> Self {
        self.variant = Some(variant);
        self
    }
}

with_common!(StatusDot);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PagerItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
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
            id: None,
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

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub items: Vec<PagerItem>,
}

impl PagerStrip {
    pub fn new(items: Vec<PagerItem>) -> Self {
        Self {
            common: CommonProps::default(),
            id: None,
            items,
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

with_common!(PagerStrip);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopoverSize {
    Small,
    #[default]
    Medium,
    Large,
    XLarge,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PopoverScaffold {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hero: Option<Box<TreeNode>>,
    pub body: Box<TreeNode>,
    pub size: PopoverSize,
}

impl PopoverScaffold {
    pub fn new(body: impl Into<TreeNode>) -> Self {
        Self {
            hero: None,
            body: Box::new(body.into()),
            size: PopoverSize::Medium,
        }
    }

    pub fn hero(mut self, hero: impl Into<TreeNode>) -> Self {
        self.hero = Some(Box::new(hero.into()));
        self
    }

    pub fn size(mut self, size: PopoverSize) -> Self {
        self.size = size;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TreeNode {
    Hero(Hero),
    Card(Card),
    Container(Container),
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
    Row(Row),
    Column(Column),
    Grid(Grid),
    Scroll(Scroll),
    LevelBar(LevelBar),
    Progress(Progress),
    Separator(Separator),
    Spinner(Spinner),
    Label(Label),
    Icon(Icon),
    Picture(Picture),
    Button(Button),
    LinkButton(LinkButton),
    Expander(Expander),
    Switch(Switch),
    ToggleButton(ToggleButton),
    Slider(Slider),
    Select(Select),
    Checkbox(Checkbox),
    PopoverScaffold(PopoverScaffold),
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
impl From<Container> for TreeNode {
    fn from(value: Container) -> Self {
        Self::Container(value)
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
impl From<Icon> for TreeNode {
    fn from(value: Icon) -> Self {
        Self::Icon(value)
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
impl From<PopoverScaffold> for TreeNode {
    fn from(value: PopoverScaffold) -> Self {
        Self::PopoverScaffold(value)
    }
}
