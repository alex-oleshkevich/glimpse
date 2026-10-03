use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use chrono::NaiveDate;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::wire::{Op, Orientation, WireNode};

pub const ROOT: u32 = 0;
pub const MAX_NODES: usize = 300;
pub const MAX_CHILDREN_PER_PARENT: usize = 200;
pub const MAX_OPS: usize = 4 * MAX_NODES;

const TEXT_CAP: usize = 256;
const NAME_CAP: usize = 64;
const BADGE_CAP: usize = 8;
const ENTRY_CAP: usize = 4096;
pub const MAX_ITEMS: usize = 64;
pub const MAX_SERIES: usize = 366;
const SERIES_KEYS: [&str; 2] = ["values", "tooltips"];
pub const MAX_SLOT_CHILDREN: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    nodes: HashMap<u32, Arc<Node>>,
    parents: HashMap<u32, u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub element: Element,
    pub children: Vec<u32>,
    pub seq: Option<u64>,
    raw: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementKind {
    Root,
    Popover,
    Footer,
    Indicator,
    Hero,
    Section,
    Row,
    SwitchRow,
    Fader,
    ActionRow,
    ToggleRow,
    MeterRow,
    ExpanderRow,
    End,
    Start,
    SliderRow,
    Entry,
    Placeholder,
    Box,
    FactList,
    ChoiceList,
    Readout,
    Notice,
    Calendar,
    DropDown,
    ToggleButton,
    CheckButton,
    Revealer,
    Stack,
    Expander,
    RangeBar,
    Bars,
    Sparkline,
    Dots,
    Label,
    Image,
    Button,
    Switch,
    Scale,
    Spinner,
    Progress,
    Separator,
    Addon,
    Container,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum ClassName {
    #[serde(rename = "dim-label")]
    DimLabel,
    #[serde(rename = "caption")]
    Caption,
    #[serde(rename = "heading")]
    Heading,
    #[serde(rename = "title-1")]
    Title1,
    #[serde(rename = "title-2")]
    Title2,
    #[serde(rename = "title-3")]
    Title3,
    #[serde(rename = "title-4")]
    Title4,
    #[serde(rename = "numeric")]
    Numeric,
    #[serde(rename = "accent")]
    Accent,
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "flat")]
    Flat,
    #[serde(rename = "pill")]
    Pill,
    #[serde(rename = "circular")]
    Circular,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Fill,
    Start,
    End,
    Center,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PopoverSize {
    #[default]
    Regular,
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Paint {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ellipsize {
    None,
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Root,
    Popover(PopoverProps),
    Footer(FooterProps),
    Indicator(IndicatorProps),
    Hero(HeroProps),
    Section(SectionProps),
    Row(RowProps),
    SwitchRow(SwitchRowProps),
    Fader(FaderProps),
    ActionRow(ActionRowProps),
    ToggleRow(ToggleRowProps),
    MeterRow(MeterRowProps),
    ExpanderRow(ExpanderRowProps),
    End(EndProps),
    Start(EndProps),
    SliderRow(SliderRowProps),
    Entry(EntryProps),
    Placeholder(PlaceholderProps),
    Box(BoxProps),
    FactList(FactListProps),
    ChoiceList(ChoiceListProps),
    Readout(ReadoutProps),
    Notice(NoticeProps),
    Calendar(CalendarProps),
    DropDown(DropDownProps),
    ToggleButton(ToggleButtonProps),
    CheckButton(CheckButtonProps),
    Revealer(RevealerProps),
    Stack(StackProps),
    Expander(ExpanderProps),
    RangeBar(RangeBarProps),
    Bars(BarsProps),
    Sparkline(SparklineProps),
    Dots(DotsProps),
    Label(LabelProps),
    Image(ImageProps),
    Button(ButtonProps),
    Switch(SwitchProps),
    Scale(ScaleProps),
    Spinner(SpinnerProps),
    Progress(ProgressProps),
    Separator(SeparatorProps),
    Addon(AddonProps),
    Container(ContainerProps),
    Unsupported(String),
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PopoverProps {
    pub class_name: Option<ClassName>,
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub size: Option<PopoverSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AddonProps {}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ContainerProps {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub background: Option<Paint>,
    pub color: Option<Paint>,
    pub align: Option<Align>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct FooterProps {
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct IndicatorProps {
    pub icon: Option<String>,
    pub text: Option<String>,
    pub tooltip: Option<String>,
    pub badge: Option<String>,
    pub overlay: Option<String>,
    pub dot: Option<String>,
    pub severity: Option<Severity>,
    pub attention: bool,
    pub notice: bool,
    pub class_name: Option<ClassName>,
    pub on_press: bool,
    pub on_scroll: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct HeroProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionProps {
    pub title: Option<String>,
    pub count: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RowProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub value: Option<String>,
    pub selected: Option<bool>,
    pub busy: bool,
    pub chevron: bool,
    pub class_name: Option<ClassName>,
    pub on_activate: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SwitchRowProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub active: bool,
    pub busy: bool,
    pub class_name: Option<ClassName>,
    pub on_toggle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct FaderProps {
    pub icon: Option<String>,
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub value: f64,
    #[serde(default = "hundred", deserialize_with = "de_finite_f64")]
    pub maximum: f64,
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub floor: f64,
    pub muted: bool,
    pub class_name: Option<ClassName>,
    pub on_change: bool,
    pub on_mute: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowTone {
    #[default]
    #[serde(skip_deserializing)]
    Normal,
    Warning,
    Error,
    Destructive,
}

fn title_lines() -> i32 {
    1
}

fn de_title_lines<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i32, D::Error> {
    let value = Value::deserialize(deserializer)?.as_f64().unwrap_or(1.0);
    Ok(
        if value.is_finite() && value.fract() == 0.0 && value >= 1.0 {
            value.min(4.0) as i32
        } else {
            1
        },
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowTypography {
    #[default]
    #[serde(skip_deserializing)]
    Default,
    Numeric,
    Monospace,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToggleControl {
    Check,
    #[default]
    Switch,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionRowProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub value: Option<String>,
    pub tone: RowTone,
    pub inactive: bool,
    pub tooltip: Option<String>,
    pub typography: RowTypography,
    pub busy: bool,
    pub on_activate: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ToggleRowProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub value: Option<String>,
    pub tone: RowTone,
    pub inactive: bool,
    pub tooltip: Option<String>,
    pub typography: RowTypography,
    pub busy: bool,
    pub control: ToggleControl,
    pub active: bool,
    pub on_toggle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct MeterRowProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub value: Option<String>,
    pub tone: RowTone,
    pub inactive: bool,
    pub tooltip: Option<String>,
    pub typography: RowTypography,
    pub busy: bool,
    #[serde(default, deserialize_with = "de_optional_f64")]
    pub level: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpanderRowProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub value: Option<String>,
    pub tone: RowTone,
    pub inactive: bool,
    pub tooltip: Option<String>,
    pub typography: RowTypography,
    pub busy: bool,
    pub open: bool,
    pub tint: bool,
    #[serde(default = "title_lines", deserialize_with = "de_title_lines")]
    pub title_lines: i32,
    pub on_toggle: bool,
}

impl Default for ExpanderRowProps {
    fn default() -> Self {
        Self {
            icon: None,
            title: None,
            subtitle: None,
            value: None,
            tone: RowTone::Normal,
            inactive: false,
            tooltip: None,
            typography: RowTypography::Default,
            busy: false,
            open: false,
            tint: false,
            title_lines: 1,
            on_toggle: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EndProps {}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SliderRowProps {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    #[serde(default, deserialize_with = "de_optional_f64")]
    pub value: Option<f64>,
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub min: f64,
    #[serde(default = "one", deserialize_with = "de_finite_f64")]
    pub max: f64,
    #[serde(default = "slider_step", deserialize_with = "de_finite_f64")]
    pub step: f64,
    pub tone: RowTone,
    pub inactive: bool,
    pub tooltip: Option<String>,
    pub on_move: bool,
    pub on_change: bool,
}

impl Default for SliderRowProps {
    fn default() -> Self {
        Self {
            title: None,
            subtitle: None,
            value: None,
            min: 0.0,
            max: 1.0,
            step: slider_step(),
            tone: RowTone::Normal,
            inactive: false,
            tooltip: None,
            on_move: false,
            on_change: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryProps {
    pub placeholder: Option<String>,
    pub value: String,
    pub class_name: Option<ClassName>,
    pub on_change: bool,
    pub on_submit: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaceholderProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct BoxProps {
    pub orientation: Orientation,
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub spacing: f64,
    pub homogeneous: bool,
    pub halign: Option<Align>,
    pub valign: Option<Align>,
    pub hexpand: bool,
    pub vexpand: bool,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LabelProps {
    pub text: Option<String>,
    pub wrap: bool,
    #[serde(default, deserialize_with = "de_optional_f64")]
    pub xalign: Option<f64>,
    pub ellipsize: Option<Ellipsize>,
    #[serde(default, deserialize_with = "de_lines")]
    pub lines: u32,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageProps {
    pub icon: Option<String>,
    pub tooltip: Option<String>,
    #[serde(default, deserialize_with = "de_pixel")]
    pub pixel_size: Option<u32>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ButtonProps {
    pub text: Option<String>,
    pub icon: Option<String>,
    pub tooltip: Option<String>,
    #[serde(default = "yes")]
    pub sensitive: bool,
    pub class_name: Option<ClassName>,
    pub on_click: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SwitchProps {
    pub active: bool,
    #[serde(default = "yes")]
    pub sensitive: bool,
    pub class_name: Option<ClassName>,
    pub on_toggle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ScaleProps {
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub value: f64,
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub min: f64,
    #[serde(default = "one", deserialize_with = "de_finite_f64")]
    pub max: f64,
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub step: f64,
    #[serde(default = "yes")]
    pub sensitive: bool,
    pub class_name: Option<ClassName>,
    pub on_change: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SpinnerProps {
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressProps {
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub fraction: f64,
    pub text: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SeparatorProps {
    pub orientation: Orientation,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Accent,
    Dim,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
pub enum Transition {
    #[serde(rename = "slide-down")]
    #[default]
    SlideDown,
    #[serde(rename = "crossfade")]
    Crossfade,
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct FactItem {
    pub label: String,
    pub value: String,
    pub warning: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct FactListProps {
    pub facts: Vec<FactItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceItem {
    pub label: String,
    pub detail: Option<String>,
    pub icon: Option<String>,
    pub warning: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceListProps {
    pub choices: Vec<ChoiceItem>,
    pub selected: Option<u32>,
    pub class_name: Option<ClassName>,
    pub on_select: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadoutProps {
    pub value: Option<String>,
    pub unit: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct NoticeProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub severity: Option<Severity>,
    pub class_name: Option<ClassName>,
    pub on_activate: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CalendarMarker {
    pub date: Option<NaiveDate>,
    pub colors: Vec<Tone>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CalendarProps {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub selected: Option<NaiveDate>,
    pub today: Option<NaiveDate>,
    pub events: Vec<CalendarMarker>,
    pub class_name: Option<ClassName>,
    pub on_month: bool,
    pub on_select: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct DropDownProps {
    pub options: Vec<String>,
    pub selected: Option<u32>,
    pub class_name: Option<ClassName>,
    pub on_select: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ToggleButtonProps {
    pub label: Option<String>,
    pub icon: Option<String>,
    pub active: bool,
    pub class_name: Option<ClassName>,
    pub on_toggle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckButtonProps {
    pub label: Option<String>,
    pub active: bool,
    pub class_name: Option<ClassName>,
    pub on_toggle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RevealerProps {
    pub revealed: bool,
    pub transition: Transition,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct StackProps {
    pub visible: u32,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpanderProps {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub expanded: bool,
    pub class_name: Option<ClassName>,
    pub on_toggle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RangeBarProps {
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub min: f64,
    #[serde(default = "one", deserialize_with = "de_finite_f64")]
    pub max: f64,
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub low: f64,
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub high: f64,
    #[serde(default, deserialize_with = "de_optional_f64")]
    pub now: Option<f64>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct BarsProps {
    pub values: Vec<Option<f64>>,
    #[serde(deserialize_with = "de_optional_f64")]
    pub maximum: Option<f64>,
    pub highlight_last: bool,
    pub label: Option<String>,
    pub tooltips: Vec<Option<String>>,
    pub class_name: Option<ClassName>,
}

impl Default for BarsProps {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            maximum: None,
            highlight_last: true,
            label: None,
            tooltips: Vec::new(),
            class_name: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SparklineProps {
    pub values: Vec<Option<f64>>,
    pub fill: bool,
    pub show_last: bool,
    pub label: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct DotsProps {
    pub colors: Vec<Tone>,
    pub uniform: bool,
    #[serde(default, deserialize_with = "de_optional_f64")]
    pub size: Option<f64>,
    pub max: Option<u32>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Violation {
    #[error("unknown id {0}")]
    UnknownId(u32),
    #[error("duplicate id {0}")]
    DuplicateId(u32),
    #[error("{id} is not a child of {parent}")]
    NotChild { parent: u32, id: u32 },
    #[error("moving {id} under {parent} would cycle")]
    Cycle { id: u32, parent: u32 },
    #[error("{parent} cannot hold {child}")]
    BadParent {
        parent: ElementKind,
        child: ElementKind,
    },
    #[error("more than 300 nodes")]
    TooManyNodes,
    #[error("parent {parent} has more than 200 children")]
    TooManyChildren { parent: u32 },
    #[error("an array prop has more than 64 items")]
    TooManyItems,
    #[error("{parent} holds a number of children its element does not allow")]
    ChildCount { parent: u32 },
    #[error("the root cannot be changed")]
    Root,
    #[error("more than 1200 operations in one commit")]
    TooManyOps,
}

impl Default for Tree {
    fn default() -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(ROOT, Arc::new(Node::root()));
        Self {
            nodes,
            parents: HashMap::new(),
        }
    }
}

impl Tree {
    pub fn apply(&self, ops: Vec<Op>) -> Result<Tree, Violation> {
        if ops.len() > MAX_OPS {
            return Err(Violation::TooManyOps);
        }
        let mut next = self.clone();
        for op in ops {
            next.apply_one(op)?;
        }
        if next.nodes.len() > MAX_NODES {
            return Err(Violation::TooManyNodes);
        }
        if let Some(parent) = next
            .nodes
            .iter()
            .find_map(|(id, node)| (node.children.len() > MAX_CHILDREN_PER_PARENT).then_some(*id))
        {
            return Err(Violation::TooManyChildren { parent });
        }
        check_child_counts(&next.nodes)?;
        Ok(next)
    }

    pub fn node(&self, id: u32) -> Option<&Arc<Node>> {
        self.nodes.get(&id)
    }

    pub fn children(&self, id: u32) -> &[u32] {
        self.nodes
            .get(&id)
            .map(|node| node.children.as_slice())
            .unwrap_or(&[])
    }

    fn apply_one(&mut self, op: Op) -> Result<(), Violation> {
        match op {
            Op::Insert {
                parent,
                node,
                before,
            } => self.insert(parent, node, before),
            Op::Move { parent, id, before } => self.move_node(parent, id, before),
            Op::Remove { parent, id } => self.remove_node(parent, id),
            Op::Set { id, props, seq } => self.set_props(id, props, seq),
        }
    }

    fn insert(
        &mut self,
        parent: u32,
        wire: WireNode,
        before: Option<u32>,
    ) -> Result<(), Violation> {
        let parent_kind = self.kind_of_node(parent)?;
        self.known_sibling(parent, before, None)?;
        let mut new_ids = HashSet::new();
        let mut built = Vec::new();
        let top = compile(wire, parent_kind, &self.nodes, &mut new_ids, &mut built)?;
        for (id, node) in built {
            for child in &node.children {
                self.parents.insert(*child, id);
            }
            self.nodes.insert(id, Arc::new(node));
        }
        self.place(parent, top, before);
        Ok(())
    }

    fn move_node(&mut self, parent: u32, id: u32, before: Option<u32>) -> Result<(), Violation> {
        if id == ROOT {
            return Err(Violation::Root);
        }
        let child_kind = self.kind_of_node(id)?;
        let parent_kind = self.kind_of_node(parent)?;
        if self.under(id, parent) {
            return Err(Violation::Cycle { id, parent });
        }
        let old = self.parent_of(id).ok_or(Violation::UnknownId(id))?;
        self.known_sibling(parent, before, Some(id))?;
        admit(parent_kind, child_kind)?;
        self.detach(old, id);
        self.place(parent, id, before);
        Ok(())
    }

    fn remove_node(&mut self, parent: u32, id: u32) -> Result<(), Violation> {
        if id == ROOT {
            return Err(Violation::Root);
        }
        self.kind_of_node(parent)?;
        if !self.nodes.contains_key(&id) {
            return Err(Violation::UnknownId(id));
        }
        if !self.is_child(parent, id) {
            return Err(Violation::NotChild { parent, id });
        }
        let drop = self.subtree(id);
        self.detach(parent, id);
        for id in drop {
            self.nodes.remove(&id);
            self.parents.remove(&id);
        }
        Ok(())
    }

    fn set_props(
        &mut self,
        id: u32,
        props: Map<String, Value>,
        seq: Option<u64>,
    ) -> Result<(), Violation> {
        if id == ROOT {
            return Err(Violation::Root);
        }
        let Some(arc) = self.nodes.get(&id).cloned() else {
            return Err(Violation::UnknownId(id));
        };
        check_items(&props)?;
        let mut node = (*arc).clone();
        for (key, value) in props {
            if value.is_null() {
                node.raw.remove(&key);
            } else {
                node.raw.insert(key, value);
            }
        }
        node.seq = seq;
        (node.element, node.raw) = rerealize(id, &node.element, &node.raw);
        self.nodes.insert(id, Arc::new(node));
        Ok(())
    }

    fn kind_of_node(&self, id: u32) -> Result<ElementKind, Violation> {
        self.nodes
            .get(&id)
            .map(|node| node.element.kind())
            .ok_or(Violation::UnknownId(id))
    }

    fn parent_of(&self, id: u32) -> Option<u32> {
        self.parents.get(&id).copied()
    }

    fn under(&self, ancestor: u32, start: u32) -> bool {
        let mut cursor = start;
        for _ in 0..=self.nodes.len() {
            if cursor == ancestor {
                return true;
            }
            match self.parent_of(cursor) {
                Some(parent) => cursor = parent,
                None => return false,
            }
        }
        true
    }

    fn is_child(&self, parent: u32, id: u32) -> bool {
        self.nodes
            .get(&parent)
            .is_some_and(|node| node.children.contains(&id))
    }

    fn known_sibling(
        &self,
        parent: u32,
        before: Option<u32>,
        moving: Option<u32>,
    ) -> Result<(), Violation> {
        let Some(before) = before else {
            return Ok(());
        };
        if moving == Some(before) || !self.is_child(parent, before) {
            return Err(if self.nodes.contains_key(&before) {
                Violation::NotChild { parent, id: before }
            } else {
                Violation::UnknownId(before)
            });
        }
        Ok(())
    }

    fn place(&mut self, parent: u32, id: u32, before: Option<u32>) {
        let Some(arc) = self.nodes.get(&parent).cloned() else {
            return;
        };
        let mut node = (*arc).clone();
        match before.and_then(|before| node.children.iter().position(|child| *child == before)) {
            Some(index) => node.children.insert(index, id),
            None => node.children.push(id),
        }
        self.nodes.insert(parent, Arc::new(node));
        self.parents.insert(id, parent);
    }

    fn detach(&mut self, parent: u32, id: u32) {
        let Some(arc) = self.nodes.get(&parent).cloned() else {
            return;
        };
        let mut node = (*arc).clone();
        node.children.retain(|child| *child != id);
        self.nodes.insert(parent, Arc::new(node));
        self.parents.remove(&id);
    }

    fn subtree(&self, id: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            out.push(id);
            if let Some(node) = self.nodes.get(&id) {
                stack.extend(node.children.iter().copied());
            }
        }
        out
    }
}

impl Node {
    fn root() -> Self {
        Self {
            element: Element::Root,
            children: Vec::new(),
            seq: None,
            raw: Map::new(),
        }
    }
}

impl Element {
    pub fn kind(&self) -> ElementKind {
        match self {
            Self::Root => ElementKind::Root,
            Self::Popover(_) => ElementKind::Popover,
            Self::Footer(_) => ElementKind::Footer,
            Self::Indicator(_) => ElementKind::Indicator,
            Self::Hero(_) => ElementKind::Hero,
            Self::Section(_) => ElementKind::Section,
            Self::Row(_) => ElementKind::Row,
            Self::SwitchRow(_) => ElementKind::SwitchRow,
            Self::Fader(_) => ElementKind::Fader,
            Self::ActionRow(_) => ElementKind::ActionRow,
            Self::ToggleRow(_) => ElementKind::ToggleRow,
            Self::MeterRow(_) => ElementKind::MeterRow,
            Self::ExpanderRow(_) => ElementKind::ExpanderRow,
            Self::End(_) => ElementKind::End,
            Self::Start(_) => ElementKind::Start,
            Self::SliderRow(_) => ElementKind::SliderRow,
            Self::Entry(_) => ElementKind::Entry,
            Self::Placeholder(_) => ElementKind::Placeholder,
            Self::Box(_) => ElementKind::Box,
            Self::FactList(_) => ElementKind::FactList,
            Self::ChoiceList(_) => ElementKind::ChoiceList,
            Self::Readout(_) => ElementKind::Readout,
            Self::Notice(_) => ElementKind::Notice,
            Self::Calendar(_) => ElementKind::Calendar,
            Self::DropDown(_) => ElementKind::DropDown,
            Self::ToggleButton(_) => ElementKind::ToggleButton,
            Self::CheckButton(_) => ElementKind::CheckButton,
            Self::Revealer(_) => ElementKind::Revealer,
            Self::Stack(_) => ElementKind::Stack,
            Self::Expander(_) => ElementKind::Expander,
            Self::RangeBar(_) => ElementKind::RangeBar,
            Self::Bars(_) => ElementKind::Bars,
            Self::Sparkline(_) => ElementKind::Sparkline,
            Self::Dots(_) => ElementKind::Dots,
            Self::Label(_) => ElementKind::Label,
            Self::Image(_) => ElementKind::Image,
            Self::Button(_) => ElementKind::Button,
            Self::Switch(_) => ElementKind::Switch,
            Self::Scale(_) => ElementKind::Scale,
            Self::Spinner(_) => ElementKind::Spinner,
            Self::Progress(_) => ElementKind::Progress,
            Self::Separator(_) => ElementKind::Separator,
            Self::Addon(_) => ElementKind::Addon,
            Self::Container(_) => ElementKind::Container,
            Self::Unsupported(_) => ElementKind::Unsupported,
        }
    }
}

impl ElementKind {
    fn name(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Popover => "popover",
            Self::Footer => "footer",
            Self::Indicator => "indicator",
            Self::Hero => "hero",
            Self::Section => "section",
            Self::Row => "row",
            Self::SwitchRow => "switchrow",
            Self::Fader => "fader",
            Self::ActionRow => "actionrow",
            Self::ToggleRow => "togglerow",
            Self::MeterRow => "meterrow",
            Self::ExpanderRow => "expanderrow",
            Self::End => "end",
            Self::Start => "start",
            Self::SliderRow => "sliderrow",
            Self::Entry => "entry",
            Self::Placeholder => "placeholder",
            Self::Box => "box",
            Self::FactList => "factlist",
            Self::ChoiceList => "choicelist",
            Self::Readout => "readout",
            Self::Notice => "notice",
            Self::Calendar => "calendar",
            Self::DropDown => "dropdown",
            Self::ToggleButton => "togglebutton",
            Self::CheckButton => "checkbutton",
            Self::Revealer => "revealer",
            Self::Stack => "stack",
            Self::Expander => "expander",
            Self::RangeBar => "rangebar",
            Self::Bars => "bars",
            Self::Sparkline => "sparkline",
            Self::Dots => "dots",
            Self::Label => "label",
            Self::Image => "image",
            Self::Button => "button",
            Self::Switch => "switch",
            Self::Scale => "scale",
            Self::Spinner => "spinner",
            Self::Progress => "progress",
            Self::Separator => "separator",
            Self::Addon => "addon",
            Self::Container => "container",
            Self::Unsupported => "unsupported",
        }
    }
}

impl fmt::Display for ElementKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

fn kind_of(name: &str) -> Option<ElementKind> {
    Some(match name {
        "popover" => ElementKind::Popover,
        "footer" => ElementKind::Footer,
        "indicator" => ElementKind::Indicator,
        "hero" => ElementKind::Hero,
        "section" => ElementKind::Section,
        "row" => ElementKind::Row,
        "switchrow" => ElementKind::SwitchRow,
        "fader" => ElementKind::Fader,
        "actionrow" => ElementKind::ActionRow,
        "togglerow" => ElementKind::ToggleRow,
        "meterrow" => ElementKind::MeterRow,
        "expanderrow" => ElementKind::ExpanderRow,
        "end" => ElementKind::End,
        "start" => ElementKind::Start,
        "sliderrow" => ElementKind::SliderRow,
        "entry" => ElementKind::Entry,
        "placeholder" => ElementKind::Placeholder,
        "box" => ElementKind::Box,
        "factlist" => ElementKind::FactList,
        "choicelist" => ElementKind::ChoiceList,
        "readout" => ElementKind::Readout,
        "notice" => ElementKind::Notice,
        "calendar" => ElementKind::Calendar,
        "dropdown" => ElementKind::DropDown,
        "togglebutton" => ElementKind::ToggleButton,
        "checkbutton" => ElementKind::CheckButton,
        "revealer" => ElementKind::Revealer,
        "stack" => ElementKind::Stack,
        "expander" => ElementKind::Expander,
        "rangebar" => ElementKind::RangeBar,
        "bars" => ElementKind::Bars,
        "sparkline" => ElementKind::Sparkline,
        "dots" => ElementKind::Dots,
        "label" => ElementKind::Label,
        "image" => ElementKind::Image,
        "button" => ElementKind::Button,
        "switch" => ElementKind::Switch,
        "scale" => ElementKind::Scale,
        "spinner" => ElementKind::Spinner,
        "progress" => ElementKind::Progress,
        "separator" => ElementKind::Separator,
        "addon" => ElementKind::Addon,
        "container" => ElementKind::Container,
        _ => return None,
    })
}

fn admit(parent: ElementKind, child: ElementKind) -> Result<(), Violation> {
    if !kind_allowed(parent, child) {
        return Err(Violation::BadParent { parent, child });
    }
    Ok(())
}

fn kind_allowed(parent: ElementKind, child: ElementKind) -> bool {
    match parent {
        ElementKind::Root => matches!(child, ElementKind::Indicator | ElementKind::Popover),
        ElementKind::Popover => {
            matches!(
                child,
                ElementKind::Hero | ElementKind::Footer | ElementKind::Addon
            ) || is_body(child)
        }
        ElementKind::Addon => matches!(
            child,
            ElementKind::Readout | ElementKind::Switch | ElementKind::Container
        ),
        ElementKind::Container => is_body(child) && child != ElementKind::Section,
        ElementKind::Section
        | ElementKind::Box
        | ElementKind::Revealer
        | ElementKind::Stack
        | ElementKind::Expander => is_body(child) && child != ElementKind::Section,
        ElementKind::Hero => matches!(
            child,
            ElementKind::Readout | ElementKind::Switch | ElementKind::Label | ElementKind::Button
        ),
        ElementKind::Row => matches!(
            child,
            ElementKind::Image | ElementKind::Label | ElementKind::Button | ElementKind::Switch
        ),
        ElementKind::ActionRow | ElementKind::MeterRow | ElementKind::End => matches!(
            child,
            ElementKind::Image | ElementKind::Readout | ElementKind::Spinner
        ),
        ElementKind::ExpanderRow => child == ElementKind::End || is_drawer_child(child),
        ElementKind::SliderRow => child == ElementKind::Start,
        ElementKind::Start => matches!(
            child,
            ElementKind::Image | ElementKind::Button | ElementKind::Readout | ElementKind::Spinner
        ),
        ElementKind::Footer => matches!(
            child,
            ElementKind::Button | ElementKind::Row | ElementKind::Label | ElementKind::Box
        ),
        _ => false,
    }
}

fn is_body(kind: ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::Section
            | ElementKind::Row
            | ElementKind::SwitchRow
            | ElementKind::Fader
            | ElementKind::ActionRow
            | ElementKind::ToggleRow
            | ElementKind::MeterRow
            | ElementKind::ExpanderRow
            | ElementKind::SliderRow
            | ElementKind::Entry
            | ElementKind::Placeholder
            | ElementKind::Box
            | ElementKind::FactList
            | ElementKind::ChoiceList
            | ElementKind::Readout
            | ElementKind::Notice
            | ElementKind::Calendar
            | ElementKind::DropDown
            | ElementKind::ToggleButton
            | ElementKind::CheckButton
            | ElementKind::Revealer
            | ElementKind::Stack
            | ElementKind::Expander
            | ElementKind::RangeBar
            | ElementKind::Bars
            | ElementKind::Sparkline
            | ElementKind::Dots
            | ElementKind::Label
            | ElementKind::Image
            | ElementKind::Button
            | ElementKind::Switch
            | ElementKind::Scale
            | ElementKind::Spinner
            | ElementKind::Progress
            | ElementKind::Separator
            | ElementKind::Container
            | ElementKind::Unsupported
    )
}

fn is_drawer_child(kind: ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::ActionRow
            | ElementKind::ToggleRow
            | ElementKind::MeterRow
            | ElementKind::SliderRow
            | ElementKind::ExpanderRow
            | ElementKind::FactList
            | ElementKind::ChoiceList
            | ElementKind::Row
    )
}

fn compile(
    wire: WireNode,
    parent_kind: ElementKind,
    existing: &HashMap<u32, Arc<Node>>,
    new_ids: &mut HashSet<u32>,
    built: &mut Vec<(u32, Node)>,
) -> Result<u32, Violation> {
    let WireNode {
        id,
        kind,
        props,
        children,
    } = wire;
    if existing.contains_key(&id) || !new_ids.insert(id) {
        return Err(Violation::DuplicateId(id));
    }
    check_items(&props)?;
    let element_kind = kind_of(&kind).unwrap_or(ElementKind::Unsupported);
    admit(parent_kind, element_kind)?;
    let mut child_ids = Vec::with_capacity(children.len());
    for child in children {
        child_ids.push(compile(child, element_kind, existing, new_ids, built)?);
    }
    let (element, raw) = realize_new(id, &kind, &props);
    built.push((
        id,
        Node {
            element,
            children: child_ids,
            seq: None,
            raw,
        },
    ));
    Ok(id)
}

fn realize_new(
    id: u32,
    type_name: &str,
    raw: &Map<String, Value>,
) -> (Element, Map<String, Value>) {
    match kind_of(type_name) {
        Some(kind) => realize_kind(id, kind, raw),
        None => (
            Element::Unsupported(glimpse_utils::clean(type_name, NAME_CAP)),
            Map::new(),
        ),
    }
}

fn rerealize(
    id: u32,
    previous: &Element,
    raw: &Map<String, Value>,
) -> (Element, Map<String, Value>) {
    match previous.kind() {
        ElementKind::Unsupported | ElementKind::Root => (previous.clone(), Map::new()),
        kind => realize_kind(id, kind, raw),
    }
}

fn realize_kind(
    id: u32,
    kind: ElementKind,
    raw: &Map<String, Value>,
) -> (Element, Map<String, Value>) {
    let mut kept = Map::new();
    let element = realize_into(id, kind, raw, &mut kept);
    (element, kept)
}

fn realize_into(
    id: u32,
    kind: ElementKind,
    raw: &Map<String, Value>,
    kept: &mut Map<String, Value>,
) -> Element {
    match kind {
        ElementKind::Root => Element::Root,
        ElementKind::Popover => Element::Popover(finish_popover(read_props(id, raw, kept))),
        ElementKind::Footer => Element::Footer(read_props(id, raw, kept)),
        ElementKind::Indicator => Element::Indicator(finish_indicator(read_props(id, raw, kept))),
        ElementKind::Hero => Element::Hero(finish_hero(read_props(id, raw, kept))),
        ElementKind::Section => Element::Section(finish_section(read_props(id, raw, kept))),
        ElementKind::Row => Element::Row(finish_row(read_props(id, raw, kept))),
        ElementKind::SwitchRow => Element::SwitchRow(finish_switch_row(read_props(id, raw, kept))),
        ElementKind::Fader => Element::Fader(finish_fader(read_props(id, raw, kept))),
        ElementKind::ActionRow => Element::ActionRow(finish_action_row(read_props(id, raw, kept))),
        ElementKind::ToggleRow => Element::ToggleRow(finish_toggle_row(read_props(id, raw, kept))),
        ElementKind::MeterRow => Element::MeterRow(finish_meter_row(read_props(id, raw, kept))),
        ElementKind::ExpanderRow => {
            Element::ExpanderRow(finish_expander_row(read_props(id, raw, kept)))
        }
        ElementKind::End => Element::End(read_props(id, raw, kept)),
        ElementKind::Start => Element::Start(read_props(id, raw, kept)),
        ElementKind::SliderRow => Element::SliderRow(finish_slider_row(read_props(id, raw, kept))),
        ElementKind::Entry => Element::Entry(finish_entry(read_props(id, raw, kept))),
        ElementKind::Placeholder => {
            Element::Placeholder(finish_placeholder(read_props(id, raw, kept)))
        }
        ElementKind::Box => Element::Box(finish_box(read_props(id, raw, kept))),
        ElementKind::FactList => Element::FactList(finish_fact_list(read_props(id, raw, kept))),
        ElementKind::ChoiceList => {
            Element::ChoiceList(finish_choice_list(read_props(id, raw, kept)))
        }
        ElementKind::Readout => Element::Readout(finish_readout(read_props(id, raw, kept))),
        ElementKind::Notice => Element::Notice(finish_notice(read_props(id, raw, kept))),
        ElementKind::Calendar => Element::Calendar(finish_calendar(read_props(id, raw, kept))),
        ElementKind::DropDown => Element::DropDown(finish_drop_down(read_props(id, raw, kept))),
        ElementKind::ToggleButton => {
            Element::ToggleButton(finish_toggle_button(read_props(id, raw, kept)))
        }
        ElementKind::CheckButton => {
            Element::CheckButton(finish_check_button(read_props(id, raw, kept)))
        }
        ElementKind::Revealer => Element::Revealer(read_props(id, raw, kept)),
        ElementKind::Stack => Element::Stack(read_props(id, raw, kept)),
        ElementKind::Expander => Element::Expander(finish_expander(read_props(id, raw, kept))),
        ElementKind::RangeBar => Element::RangeBar(finish_range_bar(read_props(id, raw, kept))),
        ElementKind::Bars => Element::Bars(finish_bars(read_props(id, raw, kept))),
        ElementKind::Sparkline => Element::Sparkline(finish_sparkline(read_props(id, raw, kept))),
        ElementKind::Dots => Element::Dots(finish_dots(read_props(id, raw, kept))),
        ElementKind::Label => Element::Label(finish_label(read_props(id, raw, kept))),
        ElementKind::Image => Element::Image(finish_image(read_props(id, raw, kept))),
        ElementKind::Button => Element::Button(finish_button(read_props(id, raw, kept))),
        ElementKind::Switch => Element::Switch(read_props(id, raw, kept)),
        ElementKind::Scale => Element::Scale(finish_scale(read_props(id, raw, kept))),
        ElementKind::Spinner => Element::Spinner(read_props(id, raw, kept)),
        ElementKind::Progress => Element::Progress(finish_progress(read_props(id, raw, kept))),
        ElementKind::Separator => Element::Separator(read_props(id, raw, kept)),
        ElementKind::Addon => Element::Addon(read_props(id, raw, kept)),
        ElementKind::Container => Element::Container(finish_container(read_props(id, raw, kept))),
        ElementKind::Unsupported => Element::Unsupported(String::new()),
    }
}

fn finish_container(props: ContainerProps) -> ContainerProps {
    ContainerProps {
        width: props.width.map(|value| value.clamp(-1, 1024)),
        height: props.height.map(|value| value.clamp(-1, 1024)),
        ..props
    }
}

fn read_props<T: DeserializeOwned + Default>(
    id: u32,
    raw: &Map<String, Value>,
    kept: &mut Map<String, Value>,
) -> T {
    for (key, value) in raw {
        if value.is_null() {
            continue;
        }
        if accepts::<T>(key, value) {
            kept.insert(key.clone(), value.clone());
        } else {
            tracing::debug!(id, key, "dropped an unknown or wrongly typed property");
        }
    }
    serde_json::from_value(Value::Object(kept.clone())).unwrap_or_default()
}

fn accepts<T: DeserializeOwned>(key: &str, value: &Value) -> bool {
    let mut trial = Map::new();
    trial.insert(key.to_owned(), value.clone());
    serde_json::from_value::<T>(Value::Object(trial)).is_ok()
}

fn finish_indicator(mut props: IndicatorProps) -> IndicatorProps {
    props.icon = icon(props.icon);
    props.overlay = icon(props.overlay);
    props.text = text(props.text);
    props.tooltip = text(props.tooltip);
    props.badge = capped(props.badge, BADGE_CAP);
    props.dot = dot(props.dot);
    props
}

fn finish_heading(
    icon_name: Option<String>,
    title: Option<String>,
    subtitle: Option<String>,
) -> (Option<String>, Option<String>, Option<String>) {
    (icon(icon_name), text(title), text(subtitle))
}

fn finish_hero(mut props: HeroProps) -> HeroProps {
    (props.icon, props.title, props.subtitle) =
        finish_heading(props.icon, props.title, props.subtitle);
    props
}

fn finish_popover(mut props: PopoverProps) -> PopoverProps {
    (props.icon, props.title, props.subtitle) =
        finish_heading(props.icon, props.title, props.subtitle);
    props
}

fn finish_section(mut props: SectionProps) -> SectionProps {
    props.title = text(props.title);
    props.count = text(props.count);
    props
}

fn finish_row(mut props: RowProps) -> RowProps {
    props.icon = icon(props.icon);
    props.title = text(props.title);
    props.subtitle = text(props.subtitle);
    props.value = text(props.value);
    props
}

fn finish_switch_row(mut props: SwitchRowProps) -> SwitchRowProps {
    props.icon = icon(props.icon);
    props.title = text(props.title);
    props.subtitle = text(props.subtitle);
    props
}

fn finish_fader(mut props: FaderProps) -> FaderProps {
    props.icon = icon(props.icon);
    props.value = finite_or(props.value, 0.0);
    props.floor = finite_or(props.floor, 0.0).max(0.0);
    props.maximum = finite_or(props.maximum, 100.0).max(props.floor);
    props
}

fn finish_row_text(
    icon_name: &mut Option<String>,
    title: &mut Option<String>,
    subtitle: &mut Option<String>,
    value: &mut Option<String>,
    tooltip: &mut Option<String>,
) {
    *icon_name = icon(icon_name.take());
    *title = row_text(title.take());
    *subtitle = row_text(subtitle.take());
    *value = row_text(value.take());
    *tooltip = row_text(tooltip.take());
}

fn finish_action_row(mut props: ActionRowProps) -> ActionRowProps {
    finish_row_text(
        &mut props.icon,
        &mut props.title,
        &mut props.subtitle,
        &mut props.value,
        &mut props.tooltip,
    );
    props
}

fn finish_toggle_row(mut props: ToggleRowProps) -> ToggleRowProps {
    finish_row_text(
        &mut props.icon,
        &mut props.title,
        &mut props.subtitle,
        &mut props.value,
        &mut props.tooltip,
    );
    props
}

fn finish_meter_row(mut props: MeterRowProps) -> MeterRowProps {
    finish_row_text(
        &mut props.icon,
        &mut props.title,
        &mut props.subtitle,
        &mut props.value,
        &mut props.tooltip,
    );
    props.level = props
        .level
        .filter(|level| level.is_finite())
        .map(|level| level.clamp(0.0, 1.0));
    props
}

fn finish_expander_row(mut props: ExpanderRowProps) -> ExpanderRowProps {
    finish_row_text(
        &mut props.icon,
        &mut props.title,
        &mut props.subtitle,
        &mut props.value,
        &mut props.tooltip,
    );
    props
}

fn finish_slider_row(mut props: SliderRowProps) -> SliderRowProps {
    props.title = row_text(props.title);
    props.subtitle = row_text(props.subtitle);
    props.tooltip = row_text(props.tooltip);
    props.min = finite_or(props.min, 0.0);
    props.max = finite_or(props.max, 1.0);
    props.step = finite_or(props.step, slider_step());
    if props.min >= props.max {
        props.min = 0.0;
        props.max = 1.0;
    }
    if props.step <= 0.0 {
        props.step = slider_step();
    }
    props.value = Some(
        props
            .value
            .filter(|value| value.is_finite())
            .unwrap_or(props.min)
            .clamp(props.min, props.max),
    );
    props
}

fn finish_entry(mut props: EntryProps) -> EntryProps {
    props.placeholder = text(props.placeholder);
    props.value = entry_value(&props.value);
    props
}

fn finish_placeholder(mut props: PlaceholderProps) -> PlaceholderProps {
    props.icon = icon(props.icon);
    props.title = text(props.title);
    props.description = text(props.description);
    props
}

fn finish_box(mut props: BoxProps) -> BoxProps {
    props.spacing = finite_or(props.spacing, 0.0).clamp(0.0, 24.0);
    props
}

fn finish_label(mut props: LabelProps) -> LabelProps {
    props.text = text(props.text);
    props.xalign = props
        .xalign
        .and_then(|value| value.is_finite().then_some(value.clamp(0.0, 1.0)));
    props
}

fn finish_image(mut props: ImageProps) -> ImageProps {
    props.icon = icon(props.icon);
    props.tooltip = text(props.tooltip);
    props
}

fn finish_button(mut props: ButtonProps) -> ButtonProps {
    props.icon = icon(props.icon);
    props.text = text(props.text);
    props.tooltip = text(props.tooltip);
    props
}

fn finish_scale(mut props: ScaleProps) -> ScaleProps {
    props.value = finite_or(props.value, 0.0);
    props.min = finite_or(props.min, 0.0);
    props.max = finite_or(props.max, 1.0);
    props.step = finite_or(props.step, 0.0).max(0.0);
    if props.min >= props.max {
        props.min = 0.0;
        props.max = 1.0;
    }
    props.value = props.value.clamp(props.min, props.max);
    props
}

fn finish_progress(mut props: ProgressProps) -> ProgressProps {
    props.fraction = finite_or(props.fraction, 0.0).clamp(0.0, 1.0);
    props.text = text(props.text);
    props
}

fn finish_fact_list(mut props: FactListProps) -> FactListProps {
    for fact in &mut props.facts {
        fact.label = clean(&fact.label);
        fact.value = clean(&fact.value);
    }
    props
}

fn finish_choice_list(mut props: ChoiceListProps) -> ChoiceListProps {
    for choice in &mut props.choices {
        choice.label = clean(&choice.label);
        choice.detail = text(choice.detail.take());
        choice.icon = icon(choice.icon.take());
    }
    props
}

fn finish_readout(mut props: ReadoutProps) -> ReadoutProps {
    props.value = text(props.value);
    props.unit = text(props.unit);
    props
}

fn finish_notice(mut props: NoticeProps) -> NoticeProps {
    props.icon = icon(props.icon);
    props.title = text(props.title);
    props.subtitle = text(props.subtitle);
    props
}

fn finish_calendar(mut props: CalendarProps) -> CalendarProps {
    props.month = props.month.map(|month| month.clamp(1, 12));
    props.year = props.year.map(|year| year.clamp(1, 9999));
    props
}

fn finish_drop_down(mut props: DropDownProps) -> DropDownProps {
    for option in &mut props.options {
        *option = clean(option);
    }
    props
}

fn finish_toggle_button(mut props: ToggleButtonProps) -> ToggleButtonProps {
    props.label = text(props.label);
    props.icon = icon(props.icon);
    props
}

fn finish_check_button(mut props: CheckButtonProps) -> CheckButtonProps {
    props.label = text(props.label);
    props
}

fn finish_expander(mut props: ExpanderProps) -> ExpanderProps {
    props.title = text(props.title);
    props.subtitle = text(props.subtitle);
    props
}

fn finish_range_bar(mut props: RangeBarProps) -> RangeBarProps {
    props.min = finite_or(props.min, 0.0);
    props.max = finite_or(props.max, 1.0);
    if props.min >= props.max {
        props.min = 0.0;
        props.max = 1.0;
    }
    props.low = finite_or(props.low, props.min).clamp(props.min, props.max);
    props.high = finite_or(props.high, props.low).clamp(props.low, props.max);
    props.now = props
        .now
        .and_then(|now| now.is_finite().then_some(now.clamp(props.min, props.max)));
    props
}

fn finish_bars(mut props: BarsProps) -> BarsProps {
    props.label = text(props.label);
    props.tooltips = props
        .tooltips
        .into_iter()
        .map(|tooltip| tooltip.map(|tooltip| clean(&tooltip)))
        .collect();
    props
}

fn finish_sparkline(mut props: SparklineProps) -> SparklineProps {
    props.label = text(props.label);
    props
}

fn finish_dots(mut props: DotsProps) -> DotsProps {
    props.size = props.size.map(|size| size.clamp(2.0, 16.0));
    props.max = props.max.map(|max| max.clamp(1, 3));
    props
}

fn clean(value: &str) -> String {
    glimpse_utils::clean(value, TEXT_CAP)
}

fn check_items(props: &Map<String, Value>) -> Result<(), Violation> {
    fn oversized(value: &Value, limit: usize) -> bool {
        match value {
            Value::Array(items) => {
                items.len() > limit || items.iter().any(|item| oversized(item, MAX_ITEMS))
            }
            Value::Object(fields) => fields.values().any(|field| oversized(field, MAX_ITEMS)),
            _ => false,
        }
    }
    let over = props.iter().any(|(key, value)| {
        let limit = if SERIES_KEYS.contains(&key.as_str()) {
            MAX_SERIES
        } else {
            MAX_ITEMS
        };
        oversized(value, limit)
    });
    if over {
        return Err(Violation::TooManyItems);
    }
    Ok(())
}

fn check_child_counts(nodes: &HashMap<u32, Arc<Node>>) -> Result<(), Violation> {
    for (id, node) in nodes {
        let count = node.children.len();
        let fits = match node.element.kind() {
            ElementKind::Hero | ElementKind::Row => count <= MAX_SLOT_CHILDREN,
            ElementKind::ActionRow
            | ElementKind::MeterRow
            | ElementKind::End
            | ElementKind::Start
            | ElementKind::SliderRow => count <= 1,
            ElementKind::ExpanderRow => {
                node.children
                    .iter()
                    .filter_map(|child| nodes.get(child))
                    .filter(|child| child.element.kind() == ElementKind::End)
                    .count()
                    <= 1
            }
            ElementKind::Addon | ElementKind::Container => count <= 1,
            ElementKind::Popover => {
                node.children
                    .iter()
                    .filter(|child| {
                        nodes
                            .get(child)
                            .is_some_and(|child| child.element.kind() == ElementKind::Addon)
                    })
                    .count()
                    <= 1
            }
            _ => true,
        };
        if !fits {
            return Err(Violation::ChildCount { parent: *id });
        }
    }
    Ok(())
}

fn text(value: Option<String>) -> Option<String> {
    capped(value, TEXT_CAP)
}

fn row_text(value: Option<String>) -> Option<String> {
    let text = text(value)?;
    if text.chars().count() <= TEXT_CAP {
        Some(text)
    } else {
        Some(text.chars().take(TEXT_CAP - 1).chain(['…']).collect())
    }
}

fn capped(value: Option<String>, cap: usize) -> Option<String> {
    let cleaned = glimpse_utils::clean(value.as_deref().unwrap_or(""), cap);
    (!cleaned.is_empty()).then_some(cleaned)
}

fn dot(value: Option<String>) -> Option<String> {
    let mut out = String::new();
    for character in value.unwrap_or_default().chars().take(TEXT_CAP) {
        out.push(character);
    }
    (!out.is_empty()).then_some(out)
}

pub(super) fn icon(value: Option<String>) -> Option<String> {
    value.filter(|name| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
    })
}

fn entry_value(value: &str) -> String {
    let mut out = String::new();
    let mut count = 0;
    for character in value.chars() {
        if hostile(character) {
            continue;
        }
        if count == ENTRY_CAP {
            break;
        }
        out.push(character);
        count += 1;
    }
    out
}

fn hostile(character: char) -> bool {
    character.is_control() || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

fn hundred() -> f64 {
    100.0
}

fn one() -> f64 {
    1.0
}

fn slider_step() -> f64 {
    0.01
}

fn yes() -> bool {
    true
}

fn de_finite_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f64::deserialize(deserializer)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("non-finite number"))
    }
}

fn de_optional_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<f64>::deserialize(deserializer)? {
        Some(value) if value.is_finite() => Ok(Some(value)),
        Some(_) => Err(serde::de::Error::custom("non-finite number")),
        None => Ok(None),
    }
}

fn de_lines<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f64::deserialize(deserializer)?;
    if !value.is_finite() || value < 0.0 {
        return Err(serde::de::Error::custom("lines"));
    }
    Ok(value.round().min(8.0) as u32)
}

fn de_pixel<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(value) = Option::<f64>::deserialize(deserializer)? else {
        return Ok(None);
    };
    if !value.is_finite() {
        return Err(serde::de::Error::custom("non-finite number"));
    }
    Ok(Some(value.round().clamp(8.0, 64.0) as u32))
}

#[cfg(test)]
mod tests;
