use crate::widget::{Key, Widget};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassName {
    DimLabel,
    Caption,
    Heading,
    Title1,
    Title2,
    Title3,
    Title4,
    Numeric,
    Accent,
    Success,
    Warning,
    Error,
    Flat,
    Pill,
    Circular,
    Column,
    NetworkPopoverRule,
    BluetoothPopover,
    NetworkPopover,
}
impl ClassName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DimLabel => "dim-label",
            Self::Caption => "caption",
            Self::Heading => "heading",
            Self::Title1 => "title-1",
            Self::Title2 => "title-2",
            Self::Title3 => "title-3",
            Self::Title4 => "title-4",
            Self::Numeric => "numeric",
            Self::Accent => "accent",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Flat => "flat",
            Self::Pill => "pill",
            Self::Circular => "circular",
            Self::Column => "column",
            Self::NetworkPopoverRule => "network-popover__rule",
            Self::BluetoothPopover => "bluetooth-popover",
            Self::NetworkPopover => "network-popover",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tone {
    Accent,
    Dim,
    Success,
    Warning,
    Error,
}
impl Tone {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::Dim => "dim",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RowTone {
    Warning,
    Error,
    Destructive,
}
impl RowTone {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Destructive => "destructive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RowTypography {
    Numeric,
    Monospace,
}
impl RowTypography {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::Monospace => "monospace",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
}
impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
impl Paint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Fill,
    Start,
    End,
    Center,
    Baseline,
}
impl Align {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::Baseline => "baseline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PopoverSize {
    Regular,
    Wide,
}
impl PopoverSize {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Wide => "wide",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToggleControl {
    Check,
    Switch,
}
impl ToggleControl {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Switch => "switch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orientation {
    Horizontal,
    Vertical,
}
impl Orientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ellipsize {
    None,
    Start,
    Middle,
    End,
}
impl Ellipsize {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Start => "start",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwatchSize {
    Regular,
    Large,
}
impl SwatchSize {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Large => "large",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transition {
    SlideDown,
    Crossfade,
    None,
}
impl Transition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SlideDown => "slide-down",
            Self::Crossfade => "crossfade",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}
impl Urgency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::Critical => "critical",
        }
    }
}

fn insert_str(map: &mut Map<String, Value>, name: &str, value: String) {
    if !value.is_empty() {
        map.insert(name.to_owned(), Value::String(value));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub value: String,
    pub warning: bool,
}

impl Fact {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            warning: false,
        }
    }

    pub fn warning(mut self, value: bool) -> Self {
        self.warning = value;
        self
    }

    fn into_value(self) -> Value {
        let mut map = Map::new();
        insert_str(&mut map, "label", self.label);
        insert_str(&mut map, "value", self.value);
        if self.warning {
            map.insert("warning".to_owned(), Value::Bool(true));
        }
        Value::Object(map)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub detail: String,
    pub icon: String,
    pub warning: bool,
}

impl Choice {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: String::new(),
            icon: String::new(),
            warning: false,
        }
    }

    pub fn detail(mut self, value: impl Into<String>) -> Self {
        self.detail = value.into();
        self
    }

    pub fn icon(mut self, value: impl Into<String>) -> Self {
        self.icon = value.into();
        self
    }

    pub fn warning(mut self, value: bool) -> Self {
        self.warning = value;
        self
    }

    fn into_value(self) -> Value {
        let mut map = Map::new();
        insert_str(&mut map, "label", self.label);
        insert_str(&mut map, "detail", self.detail);
        insert_str(&mut map, "icon", self.icon);
        if self.warning {
            map.insert("warning".to_owned(), Value::Bool(true));
        }
        Value::Object(map)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarEvent {
    pub date: String,
    pub colors: Vec<Tone>,
}

impl CalendarEvent {
    pub fn new(date: impl Into<String>) -> Self {
        Self {
            date: date.into(),
            colors: Vec::new(),
        }
    }

    pub fn colors(mut self, values: impl IntoIterator<Item = Tone>) -> Self {
        self.colors = values.into_iter().collect();
        self
    }

    pub fn color(mut self, value: Tone) -> Self {
        self.colors.push(value);
        self
    }

    fn into_value(self) -> Value {
        let mut map = Map::new();
        insert_str(&mut map, "date", self.date);
        if !self.colors.is_empty() {
            map.insert(
                "colors".to_owned(),
                Value::Array(
                    self.colors
                        .into_iter()
                        .map(|tone| Value::String(tone.as_str().to_owned()))
                        .collect(),
                ),
            );
        }
        Value::Object(map)
    }
}

pub struct Indicator<M> {
    widget: Widget<M>,
}

pub fn indicator<M>() -> Indicator<M> {
    Indicator {
        widget: Widget::new("indicator"),
    }
}

impl<M> Indicator<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn text(self, value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            widget: self.widget.text(&value),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn badge(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("badge", value.into()),
        }
    }

    pub fn overlay(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("overlay", value.into()),
        }
    }

    pub fn dot(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("dot", value.into()),
        }
    }

    pub fn severity(self, value: Severity) -> Self {
        Self {
            widget: self.widget.prop("severity", value.as_str()),
        }
    }

    pub fn attention(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("attention", value),
        }
    }

    pub fn notice(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("notice", value),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_press(self, f: impl Fn(u64) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_press(f),
        }
    }

    pub fn on_scroll(self, f: impl Fn(f64, f64) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_scroll(f),
        }
    }
}

impl<M> From<Indicator<M>> for Widget<M> {
    fn from(value: Indicator<M>) -> Self {
        value.widget
    }
}

pub struct Popover<M> {
    widget: Widget<M>,
}

pub fn popover<M>() -> Popover<M> {
    Popover {
        widget: Widget::new("popover"),
    }
}

impl<M> Popover<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn subtitle(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("subtitle", value.into()),
        }
    }

    pub fn size(self, value: PopoverSize) -> Self {
        Self {
            widget: self.widget.prop("size", value.as_str()),
        }
    }
}

impl<M> From<Popover<M>> for Widget<M> {
    fn from(value: Popover<M>) -> Self {
        value.widget
    }
}

pub struct Addon<M> {
    widget: Widget<M>,
}

pub fn addon<M>() -> Addon<M> {
    Addon {
        widget: Widget::new("addon"),
    }
}

impl<M> Addon<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }
}

impl<M> From<Addon<M>> for Widget<M> {
    fn from(value: Addon<M>) -> Self {
        value.widget
    }
}

pub struct Container<M> {
    widget: Widget<M>,
}

pub fn container<M>() -> Container<M> {
    Container {
        widget: Widget::new("container"),
    }
}

impl<M> Container<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn width(self, value: Option<i64>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("width", v),
                None => self.widget.remove_prop("width"),
            },
        }
    }

    pub fn height(self, value: Option<i64>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("height", v),
                None => self.widget.remove_prop("height"),
            },
        }
    }

    pub fn background(self, value: Paint) -> Self {
        Self {
            widget: self.widget.prop("background", value.as_str()),
        }
    }

    pub fn color(self, value: Paint) -> Self {
        Self {
            widget: self.widget.prop("color", value.as_str()),
        }
    }

    pub fn align(self, value: Align) -> Self {
        Self {
            widget: self.widget.prop("align", value.as_str()),
        }
    }
}

impl<M> From<Container<M>> for Widget<M> {
    fn from(value: Container<M>) -> Self {
        value.widget
    }
}

pub struct Section<M> {
    widget: Widget<M>,
}

pub fn section<M>() -> Section<M> {
    Section {
        widget: Widget::new("section"),
    }
}

impl<M> Section<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn count(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("count", value.into()),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Section<M>> for Widget<M> {
    fn from(value: Section<M>) -> Self {
        value.widget
    }
}

pub struct Row<M> {
    widget: Widget<M>,
}

pub fn row<M>() -> Row<M> {
    Row {
        widget: Widget::new("row"),
    }
}

impl<M> Row<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn subtitle(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("subtitle", value.into()),
        }
    }

    pub fn value(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("value", value.into()),
        }
    }

    pub fn tone(self, value: RowTone) -> Self {
        Self {
            widget: self.widget.prop("tone", value.as_str()),
        }
    }

    pub fn inactive(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("inactive", value),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn typography(self, value: RowTypography) -> Self {
        Self {
            widget: self.widget.prop("typography", value.as_str()),
        }
    }
}

impl<M> From<Row<M>> for Widget<M> {
    fn from(value: Row<M>) -> Self {
        value.widget
    }
}

pub struct ActionRow<M> {
    widget: Widget<M>,
}

pub fn action_row<M>() -> ActionRow<M> {
    ActionRow {
        widget: Widget::new("actionrow"),
    }
}

impl<M> ActionRow<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn subtitle(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("subtitle", value.into()),
        }
    }

    pub fn value(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("value", value.into()),
        }
    }

    pub fn tone(self, value: RowTone) -> Self {
        Self {
            widget: self.widget.prop("tone", value.as_str()),
        }
    }

    pub fn inactive(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("inactive", value),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn typography(self, value: RowTypography) -> Self {
        Self {
            widget: self.widget.prop("typography", value.as_str()),
        }
    }

    pub fn busy(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("busy", value),
        }
    }

    pub fn on_activate(self, f: impl Fn() -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_activate(f),
        }
    }
}

impl<M> From<ActionRow<M>> for Widget<M> {
    fn from(value: ActionRow<M>) -> Self {
        value.widget
    }
}

pub struct ToggleRow<M> {
    widget: Widget<M>,
}

pub fn toggle_row<M>() -> ToggleRow<M> {
    ToggleRow {
        widget: Widget::new("togglerow"),
    }
}

impl<M> ToggleRow<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn subtitle(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("subtitle", value.into()),
        }
    }

    pub fn value(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("value", value.into()),
        }
    }

    pub fn tone(self, value: RowTone) -> Self {
        Self {
            widget: self.widget.prop("tone", value.as_str()),
        }
    }

    pub fn inactive(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("inactive", value),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn typography(self, value: RowTypography) -> Self {
        Self {
            widget: self.widget.prop("typography", value.as_str()),
        }
    }

    pub fn busy(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("busy", value),
        }
    }

    pub fn control(self, value: ToggleControl) -> Self {
        Self {
            widget: self.widget.prop("control", value.as_str()),
        }
    }

    pub fn active(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("active", value),
        }
    }

    pub fn on_toggle(self, f: impl Fn(bool) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_toggle(f),
        }
    }
}

impl<M> From<ToggleRow<M>> for Widget<M> {
    fn from(value: ToggleRow<M>) -> Self {
        value.widget
    }
}

pub struct MeterRow<M> {
    widget: Widget<M>,
}

pub fn meter_row<M>() -> MeterRow<M> {
    MeterRow {
        widget: Widget::new("meterrow"),
    }
}

impl<M> MeterRow<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn subtitle(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("subtitle", value.into()),
        }
    }

    pub fn value(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("value", value.into()),
        }
    }

    pub fn tone(self, value: RowTone) -> Self {
        Self {
            widget: self.widget.prop("tone", value.as_str()),
        }
    }

    pub fn inactive(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("inactive", value),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn typography(self, value: RowTypography) -> Self {
        Self {
            widget: self.widget.prop("typography", value.as_str()),
        }
    }

    pub fn busy(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("busy", value),
        }
    }

    pub fn level(self, value: Option<f64>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("level", v),
                None => self.widget.remove_prop("level"),
            },
        }
    }
}

impl<M> From<MeterRow<M>> for Widget<M> {
    fn from(value: MeterRow<M>) -> Self {
        value.widget
    }
}

pub struct ExpanderRow<M> {
    widget: Widget<M>,
}

pub fn expander_row<M>() -> ExpanderRow<M> {
    ExpanderRow {
        widget: Widget::new("expanderrow"),
    }
}

impl<M> ExpanderRow<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn subtitle(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("subtitle", value.into()),
        }
    }

    pub fn value(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("value", value.into()),
        }
    }

    pub fn tone(self, value: RowTone) -> Self {
        Self {
            widget: self.widget.prop("tone", value.as_str()),
        }
    }

    pub fn inactive(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("inactive", value),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn typography(self, value: RowTypography) -> Self {
        Self {
            widget: self.widget.prop("typography", value.as_str()),
        }
    }

    pub fn busy(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("busy", value),
        }
    }

    pub fn open(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("open", value),
        }
    }

    pub fn tint(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("tint", value),
        }
    }

    pub fn title_lines(self, value: i64) -> Self {
        Self {
            widget: self.widget.prop("titleLines", value),
        }
    }

    pub fn on_toggle(self, f: impl Fn(bool) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_toggle(f),
        }
    }
}

impl<M> From<ExpanderRow<M>> for Widget<M> {
    fn from(value: ExpanderRow<M>) -> Self {
        value.widget
    }
}

pub struct Start<M> {
    widget: Widget<M>,
}

pub fn start<M>() -> Start<M> {
    Start {
        widget: Widget::new("start"),
    }
}

impl<M> Start<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }
}

impl<M> From<Start<M>> for Widget<M> {
    fn from(value: Start<M>) -> Self {
        value.widget
    }
}

pub struct End<M> {
    widget: Widget<M>,
}

pub fn end<M>() -> End<M> {
    End {
        widget: Widget::new("end"),
    }
}

impl<M> End<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }
}

impl<M> From<End<M>> for Widget<M> {
    fn from(value: End<M>) -> Self {
        value.widget
    }
}

pub struct SliderRow<M> {
    widget: Widget<M>,
}

pub fn slider_row<M>() -> SliderRow<M> {
    SliderRow {
        widget: Widget::new("sliderrow"),
    }
}

impl<M> SliderRow<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn subtitle(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("subtitle", value.into()),
        }
    }

    pub fn value(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("value", value),
        }
    }

    pub fn min(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("min", value),
        }
    }

    pub fn max(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("max", value),
        }
    }

    pub fn step(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("step", value),
        }
    }

    pub fn tone(self, value: RowTone) -> Self {
        Self {
            widget: self.widget.prop("tone", value.as_str()),
        }
    }

    pub fn inactive(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("inactive", value),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn on_move(self, f: impl Fn(f64) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_move(f),
        }
    }

    pub fn on_change(self, f: impl Fn(f64) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_change_f64(f),
        }
    }
}

impl<M> From<SliderRow<M>> for Widget<M> {
    fn from(value: SliderRow<M>) -> Self {
        value.widget
    }
}

pub struct Entry<M> {
    widget: Widget<M>,
}

pub fn entry<M>() -> Entry<M> {
    Entry {
        widget: Widget::new("entry"),
    }
}

impl<M> Entry<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn placeholder(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("placeholder", value.into()),
        }
    }

    pub fn value(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("value", value.into()),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_change(self, f: impl Fn(String) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_change_string(f),
        }
    }

    pub fn on_submit(self, f: impl Fn(String) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_submit(f),
        }
    }
}

impl<M> From<Entry<M>> for Widget<M> {
    fn from(value: Entry<M>) -> Self {
        value.widget
    }
}

pub struct EntryRow<M> {
    widget: Widget<M>,
}

pub fn entry_row<M>() -> EntryRow<M> {
    EntryRow {
        widget: Widget::new("entryrow"),
    }
}

impl<M> EntryRow<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn placeholder(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("placeholder", value.into()),
        }
    }

    pub fn value(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("value", value.into()),
        }
    }

    pub fn tone(self, value: RowTone) -> Self {
        Self {
            widget: self.widget.prop("tone", value.as_str()),
        }
    }

    pub fn inactive(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("inactive", value),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn on_change(self, f: impl Fn(String) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_change_string(f),
        }
    }

    pub fn on_submit(self, f: impl Fn(String) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_submit(f),
        }
    }
}

impl<M> From<EntryRow<M>> for Widget<M> {
    fn from(value: EntryRow<M>) -> Self {
        value.widget
    }
}

pub struct Placeholder<M> {
    widget: Widget<M>,
}

pub fn placeholder<M>() -> Placeholder<M> {
    Placeholder {
        widget: Widget::new("placeholder"),
    }
}

impl<M> Placeholder<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn description(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("description", value.into()),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Placeholder<M>> for Widget<M> {
    fn from(value: Placeholder<M>) -> Self {
        value.widget
    }
}

pub struct Bx<M> {
    widget: Widget<M>,
}

pub fn bx<M>() -> Bx<M> {
    Bx {
        widget: Widget::new("box"),
    }
}

impl<M> Bx<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn orientation(self, value: Orientation) -> Self {
        Self {
            widget: self.widget.prop("orientation", value.as_str()),
        }
    }

    pub fn spacing(self, value: i64) -> Self {
        Self {
            widget: self.widget.prop("spacing", value),
        }
    }

    pub fn homogeneous(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("homogeneous", value),
        }
    }

    pub fn halign(self, value: Align) -> Self {
        Self {
            widget: self.widget.prop("halign", value.as_str()),
        }
    }

    pub fn valign(self, value: Align) -> Self {
        Self {
            widget: self.widget.prop("valign", value.as_str()),
        }
    }

    pub fn hexpand(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("hexpand", value),
        }
    }

    pub fn vexpand(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("vexpand", value),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Bx<M>> for Widget<M> {
    fn from(value: Bx<M>) -> Self {
        value.widget
    }
}

pub struct Label<M> {
    widget: Widget<M>,
}

pub fn label<M>() -> Label<M> {
    Label {
        widget: Widget::new("label"),
    }
}

impl<M> Label<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn text(self, value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            widget: self.widget.text(&value),
        }
    }

    pub fn wrap(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("wrap", value),
        }
    }

    pub fn xalign(self, value: Option<f64>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("xalign", v),
                None => self.widget.remove_prop("xalign"),
            },
        }
    }

    pub fn ellipsize(self, value: Ellipsize) -> Self {
        Self {
            widget: self.widget.prop("ellipsize", value.as_str()),
        }
    }

    pub fn lines(self, value: i64) -> Self {
        Self {
            widget: self.widget.prop("lines", value),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Label<M>> for Widget<M> {
    fn from(value: Label<M>) -> Self {
        value.widget
    }
}

pub struct Image<M> {
    widget: Widget<M>,
}

pub fn image<M>() -> Image<M> {
    Image {
        widget: Widget::new("image"),
    }
}

impl<M> Image<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn pixel_size(self, value: Option<i64>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("pixelSize", v),
                None => self.widget.remove_prop("pixelSize"),
            },
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Image<M>> for Widget<M> {
    fn from(value: Image<M>) -> Self {
        value.widget
    }
}

pub struct Swatch<M> {
    widget: Widget<M>,
}

pub fn swatch<M>() -> Swatch<M> {
    Swatch {
        widget: Widget::new("swatch"),
    }
}

impl<M> Swatch<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn color(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("color", value.into()),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn size(self, value: SwatchSize) -> Self {
        Self {
            widget: self.widget.prop("size", value.as_str()),
        }
    }
}

impl<M> From<Swatch<M>> for Widget<M> {
    fn from(value: Swatch<M>) -> Self {
        value.widget
    }
}

pub struct Button<M> {
    widget: Widget<M>,
}

pub fn button<M>() -> Button<M> {
    Button {
        widget: Widget::new("button"),
    }
}

impl<M> Button<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn text(self, value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            widget: self.widget.text(&value),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn tooltip(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("tooltip", value.into()),
        }
    }

    pub fn sensitive(self, value: Option<bool>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("sensitive", v),
                None => self.widget.remove_prop("sensitive"),
            },
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_click(self, f: impl Fn() -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_click(f),
        }
    }
}

impl<M> From<Button<M>> for Widget<M> {
    fn from(value: Button<M>) -> Self {
        value.widget
    }
}

pub struct Switch<M> {
    widget: Widget<M>,
}

pub fn switch<M>() -> Switch<M> {
    Switch {
        widget: Widget::new("switch"),
    }
}

impl<M> Switch<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn active(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("active", value),
        }
    }

    pub fn sensitive(self, value: Option<bool>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("sensitive", v),
                None => self.widget.remove_prop("sensitive"),
            },
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_toggle(self, f: impl Fn(bool) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_toggle(f),
        }
    }
}

impl<M> From<Switch<M>> for Widget<M> {
    fn from(value: Switch<M>) -> Self {
        value.widget
    }
}

pub struct Scale<M> {
    widget: Widget<M>,
}

pub fn scale<M>() -> Scale<M> {
    Scale {
        widget: Widget::new("scale"),
    }
}

impl<M> Scale<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn value(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("value", value),
        }
    }

    pub fn min(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("min", value),
        }
    }

    pub fn max(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("max", value),
        }
    }

    pub fn step(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("step", value),
        }
    }

    pub fn sensitive(self, value: Option<bool>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("sensitive", v),
                None => self.widget.remove_prop("sensitive"),
            },
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_change(self, f: impl Fn(f64) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_change_f64(f),
        }
    }
}

impl<M> From<Scale<M>> for Widget<M> {
    fn from(value: Scale<M>) -> Self {
        value.widget
    }
}

pub struct Spinner<M> {
    widget: Widget<M>,
}

pub fn spinner<M>() -> Spinner<M> {
    Spinner {
        widget: Widget::new("spinner"),
    }
}

impl<M> Spinner<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Spinner<M>> for Widget<M> {
    fn from(value: Spinner<M>) -> Self {
        value.widget
    }
}

pub struct FactList<M> {
    widget: Widget<M>,
}

pub fn fact_list<M>() -> FactList<M> {
    FactList {
        widget: Widget::new("factlist"),
    }
}

impl<M> FactList<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn facts(self, values: impl IntoIterator<Item = Fact>) -> Self {
        Self {
            widget: self.widget.prop(
                "facts",
                Value::Array(values.into_iter().map(Fact::into_value).collect()),
            ),
        }
    }
}

impl<M> From<FactList<M>> for Widget<M> {
    fn from(value: FactList<M>) -> Self {
        value.widget
    }
}

pub struct ChoiceList<M> {
    widget: Widget<M>,
}

pub fn choice_list<M>() -> ChoiceList<M> {
    ChoiceList {
        widget: Widget::new("choicelist"),
    }
}

impl<M> ChoiceList<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn choices(self, values: impl IntoIterator<Item = Choice>) -> Self {
        Self {
            widget: self.widget.prop(
                "choices",
                Value::Array(values.into_iter().map(Choice::into_value).collect()),
            ),
        }
    }

    pub fn selected(self, value: Option<i64>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("selected", v),
                None => self.widget.remove_prop("selected"),
            },
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_select(self, f: impl Fn(usize) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_select_index(f),
        }
    }
}

impl<M> From<ChoiceList<M>> for Widget<M> {
    fn from(value: ChoiceList<M>) -> Self {
        value.widget
    }
}

pub struct Readout<M> {
    widget: Widget<M>,
}

pub fn readout<M>() -> Readout<M> {
    Readout {
        widget: Widget::new("readout"),
    }
}

impl<M> Readout<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn value(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("value", value.into()),
        }
    }

    pub fn unit(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("unit", value.into()),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Readout<M>> for Widget<M> {
    fn from(value: Readout<M>) -> Self {
        value.widget
    }
}

pub struct Notice<M> {
    widget: Widget<M>,
}

pub fn notice<M>() -> Notice<M> {
    Notice {
        widget: Widget::new("notice"),
    }
}

impl<M> Notice<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn title(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("title", value.into()),
        }
    }

    pub fn subtitle(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("subtitle", value.into()),
        }
    }

    pub fn severity(self, value: Severity) -> Self {
        Self {
            widget: self.widget.prop("severity", value.as_str()),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_activate(self, f: impl Fn() -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_activate(f),
        }
    }
}

impl<M> From<Notice<M>> for Widget<M> {
    fn from(value: Notice<M>) -> Self {
        value.widget
    }
}

pub struct Calendar<M> {
    widget: Widget<M>,
}

pub fn calendar<M>() -> Calendar<M> {
    Calendar {
        widget: Widget::new("calendar"),
    }
}

impl<M> Calendar<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn year(self, value: i64) -> Self {
        Self {
            widget: self.widget.prop("year", value),
        }
    }

    pub fn month(self, value: i64) -> Self {
        Self {
            widget: self.widget.prop("month", value),
        }
    }

    pub fn selected(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("selected", value.into()),
        }
    }

    pub fn today(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("today", value.into()),
        }
    }

    pub fn events(self, values: impl IntoIterator<Item = CalendarEvent>) -> Self {
        Self {
            widget: self.widget.prop(
                "events",
                Value::Array(values.into_iter().map(CalendarEvent::into_value).collect()),
            ),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_month(self, f: impl Fn(i64, i64) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_month(f),
        }
    }

    pub fn on_select(self, f: impl Fn(String) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_select_date(f),
        }
    }
}

impl<M> From<Calendar<M>> for Widget<M> {
    fn from(value: Calendar<M>) -> Self {
        value.widget
    }
}

pub struct Dots<M> {
    widget: Widget<M>,
}

pub fn dots<M>() -> Dots<M> {
    Dots {
        widget: Widget::new("dots"),
    }
}

impl<M> Dots<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn colors(self, values: impl IntoIterator<Item = Tone>) -> Self {
        Self {
            widget: self.widget.prop(
                "colors",
                Value::Array(
                    values
                        .into_iter()
                        .map(|v| Value::String(v.as_str().to_owned()))
                        .collect(),
                ),
            ),
        }
    }

    pub fn uniform(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("uniform", value),
        }
    }

    pub fn size(self, value: i64) -> Self {
        Self {
            widget: self.widget.prop("size", value),
        }
    }

    pub fn max(self, value: i64) -> Self {
        Self {
            widget: self.widget.prop("max", value),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Dots<M>> for Widget<M> {
    fn from(value: Dots<M>) -> Self {
        value.widget
    }
}

pub struct DropDown<M> {
    widget: Widget<M>,
}

pub fn drop_down<M>() -> DropDown<M> {
    DropDown {
        widget: Widget::new("dropdown"),
    }
}

impl<M> DropDown<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn options<S: Into<String>>(self, values: impl IntoIterator<Item = S>) -> Self {
        Self {
            widget: self.widget.prop(
                "options",
                Value::Array(
                    values
                        .into_iter()
                        .map(|v| Value::String(v.into()))
                        .collect(),
                ),
            ),
        }
    }

    pub fn selected(self, value: Option<i64>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("selected", v),
                None => self.widget.remove_prop("selected"),
            },
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_select(self, f: impl Fn(usize) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_select_index(f),
        }
    }
}

impl<M> From<DropDown<M>> for Widget<M> {
    fn from(value: DropDown<M>) -> Self {
        value.widget
    }
}

pub struct ToggleButton<M> {
    widget: Widget<M>,
}

pub fn toggle_button<M>() -> ToggleButton<M> {
    ToggleButton {
        widget: Widget::new("togglebutton"),
    }
}

impl<M> ToggleButton<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn label(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("label", value.into()),
        }
    }

    pub fn icon(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("icon", value.into()),
        }
    }

    pub fn active(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("active", value),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_toggle(self, f: impl Fn(bool) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_toggle(f),
        }
    }
}

impl<M> From<ToggleButton<M>> for Widget<M> {
    fn from(value: ToggleButton<M>) -> Self {
        value.widget
    }
}

pub struct CheckButton<M> {
    widget: Widget<M>,
}

pub fn check_button<M>() -> CheckButton<M> {
    CheckButton {
        widget: Widget::new("checkbutton"),
    }
}

impl<M> CheckButton<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn label(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("label", value.into()),
        }
    }

    pub fn active(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("active", value),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }

    pub fn on_toggle(self, f: impl Fn(bool) -> M + 'static) -> Self {
        Self {
            widget: self.widget.on_toggle(f),
        }
    }
}

impl<M> From<CheckButton<M>> for Widget<M> {
    fn from(value: CheckButton<M>) -> Self {
        value.widget
    }
}

pub struct Revealer<M> {
    widget: Widget<M>,
}

pub fn revealer<M>() -> Revealer<M> {
    Revealer {
        widget: Widget::new("revealer"),
    }
}

impl<M> Revealer<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn revealed(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("revealed", value),
        }
    }

    pub fn transition(self, value: Transition) -> Self {
        Self {
            widget: self.widget.prop("transition", value.as_str()),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Revealer<M>> for Widget<M> {
    fn from(value: Revealer<M>) -> Self {
        value.widget
    }
}

pub struct Stack<M> {
    widget: Widget<M>,
}

pub fn stack<M>() -> Stack<M> {
    Stack {
        widget: Widget::new("stack"),
    }
}

impl<M> Stack<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn visible(self, value: i64) -> Self {
        Self {
            widget: self.widget.prop("visible", value),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Stack<M>> for Widget<M> {
    fn from(value: Stack<M>) -> Self {
        value.widget
    }
}

pub struct RangeBar<M> {
    widget: Widget<M>,
}

pub fn range_bar<M>() -> RangeBar<M> {
    RangeBar {
        widget: Widget::new("rangebar"),
    }
}

impl<M> RangeBar<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn min(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("min", value),
        }
    }

    pub fn max(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("max", value),
        }
    }

    pub fn low(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("low", value),
        }
    }

    pub fn high(self, value: f64) -> Self {
        Self {
            widget: self.widget.prop("high", value),
        }
    }

    pub fn now(self, value: Option<f64>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("now", v),
                None => self.widget.remove_prop("now"),
            },
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<RangeBar<M>> for Widget<M> {
    fn from(value: RangeBar<M>) -> Self {
        value.widget
    }
}

pub struct Bars<M> {
    widget: Widget<M>,
}

pub fn bars<M>() -> Bars<M> {
    Bars {
        widget: Widget::new("bars"),
    }
}

impl<M> Bars<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn values(self, values: impl IntoIterator<Item = Option<f64>>) -> Self {
        Self {
            widget: self.widget.prop(
                "values",
                Value::Array(
                    values
                        .into_iter()
                        .map(|v| v.map(Value::from).unwrap_or(Value::Null))
                        .collect(),
                ),
            ),
        }
    }

    pub fn maximum(self, value: Option<f64>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("maximum", v),
                None => self.widget.remove_prop("maximum"),
            },
        }
    }

    pub fn highlight_last(self, value: Option<bool>) -> Self {
        Self {
            widget: match value {
                Some(v) => self.widget.prop("highlightLast", v),
                None => self.widget.remove_prop("highlightLast"),
            },
        }
    }

    pub fn label(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("label", value.into()),
        }
    }

    pub fn tooltips<S: Into<String>>(self, values: impl IntoIterator<Item = Option<S>>) -> Self {
        Self {
            widget: self.widget.prop(
                "tooltips",
                Value::Array(
                    values
                        .into_iter()
                        .map(|v| v.map(|s| Value::String(s.into())).unwrap_or(Value::Null))
                        .collect(),
                ),
            ),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Bars<M>> for Widget<M> {
    fn from(value: Bars<M>) -> Self {
        value.widget
    }
}

pub struct Sparkline<M> {
    widget: Widget<M>,
}

pub fn sparkline<M>() -> Sparkline<M> {
    Sparkline {
        widget: Widget::new("sparkline"),
    }
}

impl<M> Sparkline<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn values(self, values: impl IntoIterator<Item = Option<f64>>) -> Self {
        Self {
            widget: self.widget.prop(
                "values",
                Value::Array(
                    values
                        .into_iter()
                        .map(|v| v.map(Value::from).unwrap_or(Value::Null))
                        .collect(),
                ),
            ),
        }
    }

    pub fn fill(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("fill", value),
        }
    }

    pub fn show_last(self, value: bool) -> Self {
        Self {
            widget: self.widget.prop("showLast", value),
        }
    }

    pub fn label(self, value: impl Into<String>) -> Self {
        Self {
            widget: self.widget.prop("label", value.into()),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Sparkline<M>> for Widget<M> {
    fn from(value: Sparkline<M>) -> Self {
        value.widget
    }
}

pub struct Separator<M> {
    widget: Widget<M>,
}

pub fn separator<M>() -> Separator<M> {
    Separator {
        widget: Widget::new("separator"),
    }
}

impl<M> Separator<M> {
    pub fn key(self, key: impl Into<Key>) -> Self {
        Self {
            widget: self.widget.key(key),
        }
    }

    pub fn child(self, child: impl Into<Widget<M>>) -> Self {
        Self {
            widget: self.widget.child(child.into()),
        }
    }

    pub fn children<W: Into<Widget<M>>>(self, children: impl IntoIterator<Item = W>) -> Self {
        Self {
            widget: self.widget.children(children.into_iter().map(Into::into)),
        }
    }

    pub fn orientation(self, value: Orientation) -> Self {
        Self {
            widget: self.widget.prop("orientation", value.as_str()),
        }
    }

    pub fn class_name(self, value: ClassName) -> Self {
        Self {
            widget: self.widget.prop("className", value.as_str()),
        }
    }
}

impl<M> From<Separator<M>> for Widget<M> {
    fn from(value: Separator<M>) -> Self {
        value.widget
    }
}
