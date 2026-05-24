use std::sync::Arc;

use serde::Serialize;

#[derive(Clone)]
pub struct MsgMapper<T, Msg>(pub(crate) Arc<dyn Fn(T) -> Msg + Send + Sync>);

impl<T, Msg> MsgMapper<T, Msg> {
    pub fn new(f: impl Fn(T) -> Msg + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }
    pub(crate) fn map(&self, value: T) -> Msg {
        (self.0)(value)
    }
}

impl<T, Msg> std::fmt::Debug for MsgMapper<T, Msg> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MsgMapper(..)")
    }
}

impl<T, Msg> PartialEq for MsgMapper<T, Msg> {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeKind {
    #[default]
    Default,
    Success,
    Warning,
    Error,
    Accent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusDotStatus {
    #[default]
    Neutral,
    Success,
    Warning,
    Error,
    Accent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PagerAppearance {
    #[default]
    Dots,
    Numbers,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopoverSize {
    Small,
    #[default]
    Medium,
    Large,
    Wide,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CommonProps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub css_classes: Vec<String>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub styles: std::collections::BTreeMap<String, String>,
}

macro_rules! common_methods {
    ($ty:ty) => {
        impl $ty {
            pub fn tooltip(mut self, value: impl Into<String>) -> Self {
                self.common.tooltip = Some(value.into());
                self
            }
            pub fn css_class(mut self, value: impl Into<String>) -> Self {
                self.common.css_classes.push(value.into());
                self
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Label {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xalign: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
}
impl Label {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
            xalign: None,
            wrap: None,
        }
    }
}
common_methods!(Label);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Header {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
}
impl Header {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
        }
    }
}
common_methods!(Header);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Hero<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggle: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggle_sensitive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing: Option<Box<TreeNode<Msg>>>,
    #[serde(skip)]
    pub on_toggle: Option<MsgMapper<bool, Msg>>,
}
impl<Msg> Hero<Msg> {
    pub fn new(title: impl Into<String>, subtitle: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            id: None,
            title: title.into(),
            subtitle: subtitle.into(),
            icon: None,
            icon_size: None,
            toggle: None,
            toggle_sensitive: None,
            separator: None,
            trailing: None,
            on_toggle: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Badge {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
    pub kind: BadgeKind,
}
impl Badge {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
            kind: BadgeKind::Default,
        }
    }
}
common_methods!(Badge);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusDot {
    #[serde(flatten)]
    pub common: CommonProps,
    pub status: StatusDotStatus,
}
impl StatusDot {
    pub fn new() -> Self {
        Self {
            common: CommonProps::default(),
            status: StatusDotStatus::Neutral,
        }
    }
}
impl Default for StatusDot {
    fn default() -> Self {
        Self::new()
    }
}
common_methods!(StatusDot);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct PanelIndicator<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
    pub checked: bool,
    pub needs_attention: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<Box<TreeNode<Msg>>>,
    #[serde(skip)]
    pub on_click: Option<MsgMapper<(), Msg>>,
}
impl<Msg> PanelIndicator<Msg> {
    pub fn new() -> Self {
        Self {
            common: CommonProps::default(),
            id: None,
            icon: None,
            label: None,
            active: false,
            checked: false,
            needs_attention: false,
            extra: None,
            on_click: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmptyState {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
}
impl EmptyState {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            title: title.into(),
            subtitle: None,
        }
    }
}

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
}
impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Meter<Msg = ()> {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
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
    #[serde(skip)]
    pub on_change: Option<MsgMapper<f64, Msg>>,
}
impl<Msg> Meter<Msg> {
    pub fn new(label: impl Into<String>, value: f64) -> Self {
        Self {
            common: CommonProps::default(),
            id: None,
            icon: None,
            label: label.into(),
            value,
            min: 0.0,
            max: 1.0,
            step: 0.01,
            text: None,
            interactive: false,
            on_change: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Separator {
    #[serde(flatten)]
    pub common: CommonProps,
}
impl Separator {
    pub fn new() -> Self {
        Self {
            common: CommonProps::default(),
        }
    }
}
impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

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

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Row<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<TreeNode<Msg>>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Column<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<TreeNode<Msg>>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct BoxedList<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<TreeNode<Msg>>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct ButtonRow<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<TreeNode<Msg>>,
}
impl<Msg> Row<Msg> {
    pub fn new(children: Vec<TreeNode<Msg>>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
        }
    }
}
impl<Msg> Column<Msg> {
    pub fn new(children: Vec<TreeNode<Msg>>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
        }
    }
}
impl<Msg> BoxedList<Msg> {
    pub fn new(children: Vec<TreeNode<Msg>>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
        }
    }
}
impl<Msg> ButtonRow<Msg> {
    pub fn new(children: Vec<TreeNode<Msg>>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Container<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<TreeNode<Msg>>,
}
impl<Msg> Container<Msg> {
    pub fn new(children: Vec<TreeNode<Msg>>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CircleBox {
    #[serde(flatten)]
    pub common: CommonProps,
    pub color: String,
}
impl CircleBox {
    pub fn new(color: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            color: color.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct PopoverShell<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub size: PopoverSize,
    pub children: Vec<TreeNode<Msg>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub footer: Vec<TreeNode<Msg>>,
    #[serde(skip_serializing_if = "is_false")]
    pub footer_visible: bool,
}
impl<Msg> PopoverShell<Msg> {
    pub fn new(children: Vec<TreeNode<Msg>>) -> Self {
        Self {
            common: CommonProps::default(),
            size: PopoverSize::Medium,
            children,
            footer: Vec::new(),
            footer_visible: false,
        }
    }
}
fn is_false(value: &bool) -> bool {
    !*value
}

macro_rules! slot_structs {
    () => {
        #[derive(Debug, Clone, PartialEq, Serialize)]
        #[serde(bound(serialize = ""))]
        pub struct Tile<Msg> {
            #[serde(flatten)]
            pub common: CommonProps,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub id: Option<String>,
            pub primary: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub secondary: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left_icon: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left: Option<Box<TreeNode<Msg>>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub right: Option<Box<TreeNode<Msg>>>,
            pub activatable: bool,
            #[serde(skip)]
            pub on_click: Option<MsgMapper<(), Msg>>,
        }
        #[derive(Debug, Clone, PartialEq, Serialize)]
        #[serde(bound(serialize = ""))]
        pub struct SegmentedTile<Msg> {
            #[serde(flatten)]
            pub common: CommonProps,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub id: Option<String>,
            pub primary: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub secondary: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left_icon: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left: Option<Box<TreeNode<Msg>>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub right: Option<Box<TreeNode<Msg>>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub child: Option<Box<TreeNode<Msg>>>,
            pub expanded: bool,
            pub activatable: bool,
            #[serde(skip)]
            pub on_click: Option<MsgMapper<(), Msg>>,
            #[serde(skip)]
            pub on_toggle: Option<MsgMapper<bool, Msg>>,
        }
        #[derive(Debug, Clone, PartialEq, Serialize)]
        #[serde(bound(serialize = ""))]
        pub struct SwitchTile<Msg> {
            #[serde(flatten)]
            pub common: CommonProps,
            pub id: String,
            pub primary: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub secondary: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left_icon: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left: Option<Box<TreeNode<Msg>>>,
            pub active: bool,
            #[serde(skip)]
            pub on_toggle: Option<MsgMapper<bool, Msg>>,
        }
        #[derive(Debug, Clone, PartialEq, Serialize)]
        #[serde(bound(serialize = ""))]
        pub struct ExpanderTile<Msg> {
            #[serde(flatten)]
            pub common: CommonProps,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub id: Option<String>,
            pub primary: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub secondary: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left_icon: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left: Option<Box<TreeNode<Msg>>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub child: Option<Box<TreeNode<Msg>>>,
            pub expanded: bool,
            #[serde(skip)]
            pub on_toggle: Option<MsgMapper<bool, Msg>>,
        }
        #[derive(Debug, Clone, PartialEq, Serialize)]
        #[serde(bound(serialize = ""))]
        pub struct ChoiceTile<Msg> {
            #[serde(flatten)]
            pub common: CommonProps,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub id: Option<String>,
            pub primary: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub secondary: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left_icon: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub left: Option<Box<TreeNode<Msg>>>,
            pub selected: bool,
            #[serde(skip)]
            pub on_click: Option<MsgMapper<(), Msg>>,
        }
    };
}
slot_structs!();
impl<Msg> Tile<Msg> {
    pub fn new(primary: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            id: None,
            primary: primary.into(),
            secondary: None,
            left_icon: None,
            left: None,
            right: None,
            activatable: false,
            on_click: None,
        }
    }
}
impl<Msg> SegmentedTile<Msg> {
    pub fn new(primary: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            id: None,
            primary: primary.into(),
            secondary: None,
            left_icon: None,
            left: None,
            right: None,
            child: None,
            expanded: false,
            activatable: false,
            on_click: None,
            on_toggle: None,
        }
    }
}
impl<Msg> SwitchTile<Msg> {
    pub fn new(id: impl Into<String>, primary: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            id: id.into(),
            primary: primary.into(),
            secondary: None,
            left_icon: None,
            left: None,
            active: false,
            on_toggle: None,
        }
    }
}
impl<Msg> ExpanderTile<Msg> {
    pub fn new(primary: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            id: None,
            primary: primary.into(),
            secondary: None,
            left_icon: None,
            left: None,
            child: None,
            expanded: false,
            on_toggle: None,
        }
    }
}
impl<Msg> ChoiceTile<Msg> {
    pub fn new(primary: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            id: None,
            primary: primary.into(),
            secondary: None,
            left_icon: None,
            left: None,
            selected: false,
            on_click: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct SliderTile<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode<Msg>>>,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub page: f64,
    pub digits: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snap_step: Option<f64>,
    #[serde(skip)]
    pub on_change: Option<MsgMapper<f64, Msg>>,
}
impl<Msg> SliderTile<Msg> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            id: id.into(),
            label: None,
            left_icon: None,
            left: None,
            value: 0.0,
            min: 0.0,
            max: 1.0,
            step: 0.01,
            page: 0.1,
            digits: 0,
            snap_step: None,
            on_change: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Choice {
    pub id: String,
    pub primary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct ChoiceList<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    pub choices: Vec<Choice>,
    #[serde(skip)]
    pub on_change: Option<MsgMapper<String, Msg>>,
}
impl<Msg> ChoiceList<Msg> {
    pub fn new(id: impl Into<String>, choices: Vec<Choice>) -> Self {
        Self {
            common: CommonProps::default(),
            id: id.into(),
            active: None,
            choices,
            on_change: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyValueRow {
    pub key: String,
    pub value: String,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeyValueGrid {
    #[serde(flatten)]
    pub common: CommonProps,
    pub rows: Vec<KeyValueRow>,
}
impl KeyValueGrid {
    pub fn new(rows: Vec<KeyValueRow>) -> Self {
        Self {
            common: CommonProps::default(),
            rows,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct PagerItem<Msg = ()> {
    #[serde(flatten)]
    pub common: CommonProps,
    pub id: u64,
    pub label: String,
    pub appearance: PagerAppearance,
    pub active: bool,
    pub inactive: bool,
    pub occupied: bool,
    pub urgent: bool,
    #[serde(skip)]
    pub on_click: Option<MsgMapper<(), Msg>>,
}
impl<Msg> PagerItem<Msg> {
    pub fn new(id: u64) -> Self {
        Self {
            common: CommonProps::default(),
            id,
            label: String::new(),
            appearance: PagerAppearance::Dots,
            active: false,
            inactive: false,
            occupied: false,
            urgent: false,
            on_click: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct PagerStrip<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub placeholder: bool,
    pub items: Vec<PagerItem<Msg>>,
    #[serde(skip)]
    pub on_change: Option<MsgMapper<u64, Msg>>,
}
impl<Msg> PagerStrip<Msg> {
    pub fn new(items: Vec<PagerItem<Msg>>) -> Self {
        Self {
            common: CommonProps::default(),
            id: None,
            placeholder: false,
            items,
            on_change: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActiveIndicator {
    #[serde(flatten)]
    pub common: CommonProps,
    pub active: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CameraIndicator {
    #[serde(flatten)]
    pub data: ActiveIndicator,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MicIndicator {
    #[serde(flatten)]
    pub data: ActiveIndicator,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MutedIndicator {
    #[serde(flatten)]
    pub data: ActiveIndicator,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocationIndicator {
    #[serde(flatten)]
    pub data: ActiveIndicator,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScreenCastIndicator {
    #[serde(flatten)]
    pub data: ActiveIndicator,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer_text: Option<String>,
}

impl ActiveIndicator {
    pub fn active() -> Self {
        Self {
            common: CommonProps::default(),
            active: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Calendar<Msg> {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub selected_date: String,
    pub event_days: Vec<String>,
    #[serde(skip)]
    pub on_change: Option<MsgMapper<String, Msg>>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatteryHero {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: String,
    pub percentage: String,
    pub fraction: f64,
    pub state: String,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DateHero {
    #[serde(flatten)]
    pub common: CommonProps,
    pub weekday: String,
    pub date: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventItem {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub all_day: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Events {
    #[serde(flatten)]
    pub common: CommonProps,
    pub date: String,
    pub events: Vec<EventItem>,
    pub loading: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WeatherForecastItem {
    pub day_name: String,
    pub icon: String,
    pub condition: String,
    pub temperatures: String,
    pub is_today: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeatherForecastList {
    #[serde(flatten)]
    pub common: CommonProps,
    pub items: Vec<WeatherForecastItem>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WeatherHourlyItem {
    pub time: String,
    pub icon: String,
    pub temperature: String,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeatherHourlyStrip {
    #[serde(flatten)]
    pub common: CommonProps,
    pub items: Vec<WeatherHourlyItem>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorldClockRow {
    pub name: String,
    pub timezone: String,
    pub time: String,
    pub offset: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_label: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorldClock {
    #[serde(flatten)]
    pub common: CommonProps,
    pub rows: Vec<WorldClockRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(bound(serialize = ""))]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TreeNode<Msg> {
    Row(Row<Msg>),
    Column(Column<Msg>),
    Container(Container<Msg>),
    CircleBox(CircleBox),
    BoxedList(BoxedList<Msg>),
    PopoverShell(PopoverShell<Msg>),
    Label(Label),
    Header(Header),
    Hero(Hero<Msg>),
    Badge(Badge),
    StatusDot(StatusDot),
    PanelIndicator(PanelIndicator<Msg>),
    EmptyState(EmptyState),
    Spinner(Spinner),
    Meter(Meter<Msg>),
    Separator(Separator),
    Scroll(Scroll<Msg>),
    Tile(Tile<Msg>),
    SegmentedTile(SegmentedTile<Msg>),
    ButtonRow(ButtonRow<Msg>),
    SwitchTile(SwitchTile<Msg>),
    ExpanderTile(ExpanderTile<Msg>),
    SliderTile(SliderTile<Msg>),
    ChoiceTile(ChoiceTile<Msg>),
    ChoiceList(ChoiceList<Msg>),
    KeyValueGrid(KeyValueGrid),
    PagerItem(PagerItem<Msg>),
    PagerStrip(PagerStrip<Msg>),
    CameraIndicator(CameraIndicator),
    MicIndicator(MicIndicator),
    MutedIndicator(MutedIndicator),
    #[serde(rename = "screencast_indicator")]
    ScreenCastIndicator(ScreenCastIndicator),
    LocationIndicator(LocationIndicator),
    Calendar(Calendar<Msg>),
    BatteryHero(BatteryHero),
    DateHero(DateHero),
    Events(Events),
    WeatherForecastList(WeatherForecastList),
    WeatherHourlyStrip(WeatherHourlyStrip),
    WorldClock(WorldClock),
}

macro_rules! from_node {
    ($($ty:ty => $variant:ident),+ $(,)?) => {$(
        impl<Msg> From<$ty> for TreeNode<Msg> {
            fn from(v: $ty) -> Self { Self::$variant(v) }
        }
    )+};
}

from_node!(
    Row<Msg> => Row, Column<Msg> => Column, Container<Msg> => Container, BoxedList<Msg> => BoxedList,
    PopoverShell<Msg> => PopoverShell, Hero<Msg> => Hero, PanelIndicator<Msg> => PanelIndicator,
    Tile<Msg> => Tile, SegmentedTile<Msg> => SegmentedTile, ButtonRow<Msg> => ButtonRow,
    SwitchTile<Msg> => SwitchTile, ExpanderTile<Msg> => ExpanderTile, SliderTile<Msg> => SliderTile,
    ChoiceTile<Msg> => ChoiceTile, ChoiceList<Msg> => ChoiceList, PagerItem<Msg> => PagerItem,
    PagerStrip<Msg> => PagerStrip, Calendar<Msg> => Calendar, Meter<Msg> => Meter, Scroll<Msg> => Scroll,
);

macro_rules! from_static_node {
    ($($ty:ty => $variant:ident),+ $(,)?) => {$(
        impl<Msg> From<$ty> for TreeNode<Msg> {
            fn from(v: $ty) -> Self { Self::$variant(v) }
        }
    )+};
}

from_static_node!(
    CircleBox => CircleBox,
    Label => Label, Header => Header, Badge => Badge, StatusDot => StatusDot, EmptyState => EmptyState,
    Spinner => Spinner, Separator => Separator,
    KeyValueGrid => KeyValueGrid, CameraIndicator => CameraIndicator, MicIndicator => MicIndicator,
    MutedIndicator => MutedIndicator, ScreenCastIndicator => ScreenCastIndicator,
    LocationIndicator => LocationIndicator, BatteryHero => BatteryHero, DateHero => DateHero,
    Events => Events, WeatherForecastList => WeatherForecastList, WeatherHourlyStrip => WeatherHourlyStrip,
    WorldClock => WorldClock,
);
