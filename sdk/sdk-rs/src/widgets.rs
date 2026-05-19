use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

// Wraps a sync mapping function used for value-carrying events (toggle, change).
// Always equal for tree diffing — handler identity doesn't affect the diff.
pub struct MsgMapper<T, Msg>(pub(crate) Arc<dyn Fn(T) -> Msg + Send + Sync>);

impl<T, Msg> Clone for MsgMapper<T, Msg> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T, Msg> PartialEq for MsgMapper<T, Msg> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T, Msg> std::fmt::Debug for MsgMapper<T, Msg> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MsgMapper").finish()
    }
}

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
pub enum TextAlign {
    Left,
    Center,
    Right,
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
    ($name:ident < Msg >) => {
        impl<Msg> $name<Msg> {
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
pub struct Text {
    #[serde(flatten)]
    pub common: CommonProps,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<FontSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<FontWeight>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlign>,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            text: text.into(),
            color: None,
            size: None,
            weight: None,
            align: None,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn size(mut self, size: FontSize) -> Self {
        self.size = Some(size);
        self
    }

    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = Some(align);
        self
    }
}

with_common!(Text);

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
#[serde(bound(serialize = ""))]
pub struct Button<Msg> {
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
    #[serde(skip)]
    pub(crate) on_click: Option<Msg>,
}

impl<Msg> Button<Msg> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
            label: None,
            icon: None,
            enabled: None,
            variant: None,
            on_click: None,
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

    pub fn on_click(mut self, msg: Msg) -> Self {
        self.on_click = Some(msg);
        self
    }
}

with_common!(Button<Msg>);

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
#[serde(bound(serialize = ""))]
pub struct Expander<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
    pub expanded: bool,
    pub child: Box<TreeNode<Msg>>,
}

impl<Msg> Expander<Msg> {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
            expanded: false,
            child: Box::new(Text::new("").into()),
        }
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn child(mut self, child: impl Into<TreeNode<Msg>>) -> Self {
        self.child = Box::new(child.into());
        self
    }
}

with_common!(Expander<Msg>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Switch<Msg> {
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
    #[serde(skip)]
    pub(crate) on_toggle: Option<MsgMapper<bool, Msg>>,
}

impl<Msg> Switch<Msg> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
            label: None,
            active: false,
            on_toggle: None,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn on_toggle<F: Fn(bool) -> Msg + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_toggle = Some(MsgMapper(Arc::new(f)));
        self
    }
}

with_common!(Switch<Msg>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct ToggleButton<Msg> {
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub active: bool,
    #[serde(skip)]
    pub(crate) on_toggle: Option<MsgMapper<bool, Msg>>,
}

impl<Msg> ToggleButton<Msg> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
            label: None,
            icon: None,
            active: false,
            on_toggle: None,
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

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn on_toggle<F: Fn(bool) -> Msg + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_toggle = Some(MsgMapper(Arc::new(f)));
        self
    }
}

with_common!(ToggleButton<Msg>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Slider<Msg> {
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
    #[serde(skip)]
    pub(crate) on_change: Option<MsgMapper<Option<serde_json::Value>, Msg>>,
}

impl<Msg> Slider<Msg> {
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
            on_change: None,
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

    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }

    pub fn on_change<F: Fn(Option<serde_json::Value>) -> Msg + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_change = Some(MsgMapper(Arc::new(f)));
        self
    }
}

with_common!(Slider<Msg>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Checkbox<Msg> {
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
    #[serde(skip)]
    pub(crate) on_toggle: Option<MsgMapper<bool, Msg>>,
}

impl<Msg> Checkbox<Msg> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
            label: None,
            active: false,
            on_toggle: None,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn on_toggle<F: Fn(bool) -> Msg + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_toggle = Some(MsgMapper(Arc::new(f)));
        self
    }
}

with_common!(Checkbox<Msg>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Select<Msg> {
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(serialize_with = "serialize_select_items")]
    pub items: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<u32>,
    #[serde(skip)]
    pub(crate) on_change: Option<MsgMapper<Option<serde_json::Value>, Msg>>,
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

impl<Msg> Select<Msg> {
    pub fn new(id: impl Into<String>, items: Vec<(String, String)>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
            items,
            selected: None,
            on_change: None,
        }
    }

    pub fn selected(mut self, selected: u32) -> Self {
        self.selected = Some(selected);
        self
    }

    pub fn on_change<F: Fn(Option<serde_json::Value>) -> Msg + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_change = Some(MsgMapper(Arc::new(f)));
        self
    }
}

with_common!(Select<Msg>);

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

impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

with_common!(Separator);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Scroll<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode<Msg>>,
}

impl<Msg> Scroll<Msg> {
    pub fn new(child: TreeNode<Msg>) -> Self {
        Self {
            common: CommonProps::default(),
            child: Box::new(child),
        }
    }
}

with_common!(Scroll<Msg>);

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
#[serde(bound(serialize = ""))]
pub struct GridChild<Msg> {
    pub row: i32,
    pub column: i32,
    pub width: i32,
    pub height: i32,
    pub child: TreeNode<Msg>,
}

impl<Msg> GridChild<Msg> {
    pub fn new(row: i32, column: i32, child: TreeNode<Msg>) -> Self {
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
#[serde(bound(serialize = ""))]
pub struct Grid<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<GridChild<Msg>>,
    pub row_spacing: i32,
    pub column_spacing: i32,
}

impl<Msg> Grid<Msg> {
    pub fn new(children: Vec<GridChild<Msg>>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
            row_spacing: 4,
            column_spacing: 4,
        }
    }
}

with_common!(Grid<Msg>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Hero<Msg> {
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
    #[serde(skip)]
    pub(crate) on_toggle: Option<MsgMapper<bool, Msg>>,
}

impl<Msg> Hero<Msg> {
    pub fn new(title: impl Into<String>, subtitle: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            icon: None,
            id: None,
            switch: None,
            common: CommonProps::default(),
            on_toggle: None,
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

    pub fn on_toggle<F: Fn(bool) -> Msg + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_toggle = Some(MsgMapper(Arc::new(f)));
        self
    }
}

with_common!(Hero<Msg>);

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
#[serde(bound(serialize = ""))]
pub struct Card<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<TreeNode<Msg>>>,
}

impl<Msg> Card<Msg> {
    pub fn new(child: Option<TreeNode<Msg>>) -> Self {
        Self {
            common: CommonProps::default(),
            child: child.map(Box::new),
        }
    }
}

with_common!(Card<Msg>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Container<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<TreeNode<Msg>>>,
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

impl<Msg> Container<Msg> {
    pub fn new(child: Option<TreeNode<Msg>>) -> Self {
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

    pub fn child(mut self, child: impl Into<TreeNode<Msg>>) -> Self {
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

with_common!(Container<Msg>);

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
#[serde(bound(serialize = ""))]
pub struct Row<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub spacing: i32,
    pub children: Vec<TreeNode<Msg>>,
}

impl<Msg> Row<Msg> {
    pub fn new(children: Vec<TreeNode<Msg>>) -> Self {
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

with_common!(Row<Msg>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Column<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub spacing: i32,
    pub children: Vec<TreeNode<Msg>>,
}

impl<Msg> Column<Msg> {
    pub fn new(children: Vec<TreeNode<Msg>>) -> Self {
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

with_common!(Column<Msg>);

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
#[serde(bound(serialize = ""))]
pub struct Item<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode<Msg>>>,
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sublabel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<Box<TreeNode<Msg>>>,
}

impl<Msg> Item<Msg> {
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
        self.left = Some(Box::new(TreeNode::Icon(Icon {
            common: CommonProps::default(),
            icon: name.into(),
            pixel_size: Some(16),
        })));
        self
    }

    pub fn left(mut self, left: impl Into<TreeNode<Msg>>) -> Self {
        self.left = Some(Box::new(left.into()));
        self
    }

    pub fn sublabel(mut self, sublabel: impl Into<String>) -> Self {
        self.sublabel = sublabel.into();
        self
    }

    pub fn right(mut self, right: impl Into<TreeNode<Msg>>) -> Self {
        self.right = Some(Box::new(right.into()));
        self
    }
}

with_common!(Item<Msg>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct ActionItem<Msg> {
    pub id: String,
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode<Msg>>>,
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sublabel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<Box<TreeNode<Msg>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip)]
    pub(crate) on_click: Option<Msg>,
}

impl<Msg> ActionItem<Msg> {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            common: CommonProps::default(),
            left: None,
            label: label.into(),
            sublabel: String::new(),
            right: None,
            enabled: None,
            on_click: None,
        }
    }

    pub fn icon(mut self, name: impl Into<String>) -> Self {
        self.left = Some(Box::new(TreeNode::Icon(Icon {
            common: CommonProps::default(),
            icon: name.into(),
            pixel_size: Some(16),
        })));
        self
    }

    pub fn left(mut self, left: impl Into<TreeNode<Msg>>) -> Self {
        self.left = Some(Box::new(left.into()));
        self
    }

    pub fn sublabel(mut self, sublabel: impl Into<String>) -> Self {
        self.sublabel = sublabel.into();
        self
    }

    pub fn right(mut self, right: impl Into<TreeNode<Msg>>) -> Self {
        self.right = Some(Box::new(right.into()));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    pub fn on_click(mut self, msg: Msg) -> Self {
        self.on_click = Some(msg);
        self
    }
}

with_common!(ActionItem<Msg>);

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

impl Default for StatusDot {
    fn default() -> Self {
        Self::new()
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
#[serde(bound(serialize = ""))]
pub struct PopoverScaffold<Msg> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hero: Option<Box<TreeNode<Msg>>>,
    pub body: Box<TreeNode<Msg>>,
    pub size: PopoverSize,
}

impl<Msg> PopoverScaffold<Msg> {
    pub fn new(body: impl Into<TreeNode<Msg>>) -> Self {
        Self {
            hero: None,
            body: Box::new(body.into()),
            size: PopoverSize::Medium,
        }
    }

    pub fn hero(mut self, hero: impl Into<TreeNode<Msg>>) -> Self {
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
#[serde(bound(serialize = ""))]
pub enum TreeNode<Msg> {
    Hero(Hero<Msg>),
    Card(Card<Msg>),
    Container(Container<Msg>),
    Meter(Meter),
    Copyable(Copyable),
    PropertyList(PropertyList),
    Item(Item<Msg>),
    ActionItem(ActionItem<Msg>),
    EmptyState(EmptyState),
    Badge(Badge),
    #[serde(rename = "status")]
    StatusDot(StatusDot),
    PagerItem(PagerItem),
    PagerStrip(PagerStrip),
    Row(Row<Msg>),
    Column(Column<Msg>),
    Grid(Grid<Msg>),
    Scroll(Scroll<Msg>),
    LevelBar(LevelBar),
    Progress(Progress),
    Separator(Separator),
    Spinner(Spinner),
    Text(Text),
    Icon(Icon),
    Picture(Picture),
    Button(Button<Msg>),
    LinkButton(LinkButton),
    Expander(Expander<Msg>),
    Switch(Switch<Msg>),
    ToggleButton(ToggleButton<Msg>),
    Slider(Slider<Msg>),
    Select(Select<Msg>),
    Checkbox(Checkbox<Msg>),
    PopoverScaffold(PopoverScaffold<Msg>),
}

impl<Msg> From<Hero<Msg>> for TreeNode<Msg> {
    fn from(v: Hero<Msg>) -> Self { Self::Hero(v) }
}
impl<Msg> From<Card<Msg>> for TreeNode<Msg> {
    fn from(v: Card<Msg>) -> Self { Self::Card(v) }
}
impl<Msg> From<Container<Msg>> for TreeNode<Msg> {
    fn from(v: Container<Msg>) -> Self { Self::Container(v) }
}
impl<Msg> From<Meter> for TreeNode<Msg> {
    fn from(v: Meter) -> Self { Self::Meter(v) }
}
impl<Msg> From<Copyable> for TreeNode<Msg> {
    fn from(v: Copyable) -> Self { Self::Copyable(v) }
}
impl<Msg> From<Row<Msg>> for TreeNode<Msg> {
    fn from(v: Row<Msg>) -> Self { Self::Row(v) }
}
impl<Msg> From<Column<Msg>> for TreeNode<Msg> {
    fn from(v: Column<Msg>) -> Self { Self::Column(v) }
}
impl<Msg> From<Spinner> for TreeNode<Msg> {
    fn from(v: Spinner) -> Self { Self::Spinner(v) }
}
impl<Msg> From<PropertyList> for TreeNode<Msg> {
    fn from(v: PropertyList) -> Self { Self::PropertyList(v) }
}
impl<Msg> From<Item<Msg>> for TreeNode<Msg> {
    fn from(v: Item<Msg>) -> Self { Self::Item(v) }
}
impl<Msg> From<ActionItem<Msg>> for TreeNode<Msg> {
    fn from(v: ActionItem<Msg>) -> Self { Self::ActionItem(v) }
}
impl<Msg> From<EmptyState> for TreeNode<Msg> {
    fn from(v: EmptyState) -> Self { Self::EmptyState(v) }
}
impl<Msg> From<Badge> for TreeNode<Msg> {
    fn from(v: Badge) -> Self { Self::Badge(v) }
}
impl<Msg> From<StatusDot> for TreeNode<Msg> {
    fn from(v: StatusDot) -> Self { Self::StatusDot(v) }
}
impl<Msg> From<PagerItem> for TreeNode<Msg> {
    fn from(v: PagerItem) -> Self { Self::PagerItem(v) }
}
impl<Msg> From<PagerStrip> for TreeNode<Msg> {
    fn from(v: PagerStrip) -> Self { Self::PagerStrip(v) }
}
impl<Msg> From<Grid<Msg>> for TreeNode<Msg> {
    fn from(v: Grid<Msg>) -> Self { Self::Grid(v) }
}
impl<Msg> From<Scroll<Msg>> for TreeNode<Msg> {
    fn from(v: Scroll<Msg>) -> Self { Self::Scroll(v) }
}
impl<Msg> From<LevelBar> for TreeNode<Msg> {
    fn from(v: LevelBar) -> Self { Self::LevelBar(v) }
}
impl<Msg> From<Progress> for TreeNode<Msg> {
    fn from(v: Progress) -> Self { Self::Progress(v) }
}
impl<Msg> From<Separator> for TreeNode<Msg> {
    fn from(v: Separator) -> Self { Self::Separator(v) }
}
impl<Msg> From<Text> for TreeNode<Msg> {
    fn from(v: Text) -> Self { Self::Text(v) }
}
impl<Msg> From<Icon> for TreeNode<Msg> {
    fn from(v: Icon) -> Self { Self::Icon(v) }
}
impl<Msg> From<Picture> for TreeNode<Msg> {
    fn from(v: Picture) -> Self { Self::Picture(v) }
}
impl<Msg> From<Button<Msg>> for TreeNode<Msg> {
    fn from(v: Button<Msg>) -> Self { Self::Button(v) }
}
impl<Msg> From<LinkButton> for TreeNode<Msg> {
    fn from(v: LinkButton) -> Self { Self::LinkButton(v) }
}
impl<Msg> From<Expander<Msg>> for TreeNode<Msg> {
    fn from(v: Expander<Msg>) -> Self { Self::Expander(v) }
}
impl<Msg> From<Switch<Msg>> for TreeNode<Msg> {
    fn from(v: Switch<Msg>) -> Self { Self::Switch(v) }
}
impl<Msg> From<ToggleButton<Msg>> for TreeNode<Msg> {
    fn from(v: ToggleButton<Msg>) -> Self { Self::ToggleButton(v) }
}
impl<Msg> From<Slider<Msg>> for TreeNode<Msg> {
    fn from(v: Slider<Msg>) -> Self { Self::Slider(v) }
}
impl<Msg> From<Select<Msg>> for TreeNode<Msg> {
    fn from(v: Select<Msg>) -> Self { Self::Select(v) }
}
impl<Msg> From<Checkbox<Msg>> for TreeNode<Msg> {
    fn from(v: Checkbox<Msg>) -> Self { Self::Checkbox(v) }
}
impl<Msg> From<PopoverScaffold<Msg>> for TreeNode<Msg> {
    fn from(v: PopoverScaffold<Msg>) -> Self { Self::PopoverScaffold(v) }
}
