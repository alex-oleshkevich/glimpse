use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, LazyLock, Mutex};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::wire::{Op, Orientation, WireNode};

pub const ROOT: u32 = 0;
pub const MAX_NODES: usize = 300;
pub const MAX_CHILDREN_PER_PARENT: usize = 200;

const TEXT_CAP: usize = 256;
const NAME_CAP: usize = 64;
const BADGE_CAP: usize = 8;
const ENTRY_CAP: usize = 4096;

#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    nodes: HashMap<u32, Arc<Node>>,
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
    Entry,
    Placeholder,
    Box,
    Label,
    Image,
    Button,
    Switch,
    Scale,
    Spinner,
    Progress,
    Separator,
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
    Entry(EntryProps),
    Placeholder(PlaceholderProps),
    Box(BoxProps),
    Label(LabelProps),
    Image(ImageProps),
    Button(ButtonProps),
    Switch(SwitchProps),
    Scale(ScaleProps),
    Spinner(SpinnerProps),
    Progress(ProgressProps),
    Separator(SeparatorProps),
    Unsupported(String),
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PopoverProps {
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FooterProps {
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
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
#[serde(default, rename_all = "camelCase")]
pub struct HeroProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SectionProps {
    pub title: Option<String>,
    pub count: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RowProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub value: Option<String>,
    pub selected: Option<bool>,
    pub busy: bool,
    pub class_name: Option<ClassName>,
    pub on_activate: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
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
#[serde(default, rename_all = "camelCase")]
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

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EntryProps {
    pub placeholder: Option<String>,
    pub value: String,
    pub class_name: Option<ClassName>,
    pub on_change: bool,
    pub on_submit: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlaceholderProps {
    pub icon: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
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
#[serde(default, rename_all = "camelCase")]
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
#[serde(default, rename_all = "camelCase")]
pub struct ImageProps {
    pub icon: Option<String>,
    pub tooltip: Option<String>,
    #[serde(default, deserialize_with = "de_pixel")]
    pub pixel_size: Option<u32>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
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
#[serde(default, rename_all = "camelCase")]
pub struct SwitchProps {
    pub active: bool,
    #[serde(default = "yes")]
    pub sensitive: bool,
    pub class_name: Option<ClassName>,
    pub on_toggle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
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
#[serde(default, rename_all = "camelCase")]
pub struct SpinnerProps {
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProgressProps {
    #[serde(default, deserialize_with = "de_finite_f64")]
    pub fraction: f64,
    pub text: Option<String>,
    pub class_name: Option<ClassName>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SeparatorProps {
    pub orientation: Orientation,
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
    #[error("{parent} already holds a {child}")]
    AlreadyHolds {
        parent: ElementKind,
        child: ElementKind,
    },
    #[error("more than 300 nodes")]
    TooManyNodes,
    #[error("parent {parent} has more than 200 children")]
    TooManyChildren { parent: u32 },
    #[error("the root cannot be changed")]
    Root,
}

impl Default for Tree {
    fn default() -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(ROOT, Arc::new(Node::root()));
        Self { nodes }
    }
}

impl Tree {
    pub fn apply(&self, ops: Vec<Op>) -> Result<Tree, Violation> {
        let mut next = self.clone();
        for op in ops {
            next.apply_one(op)?;
        }
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
        let siblings = self.child_kinds(parent);
        let mut new_ids = HashSet::new();
        let mut built = Vec::new();
        let top = compile(
            wire,
            parent_kind,
            &siblings,
            &self.nodes,
            &mut new_ids,
            &mut built,
        )?;
        if self.child_count(parent) + 1 > MAX_CHILDREN_PER_PARENT {
            return Err(Violation::TooManyChildren { parent });
        }
        if self.nodes.len() + new_ids.len() > MAX_NODES {
            return Err(Violation::TooManyNodes);
        }
        for (id, node) in built {
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
        let siblings = self.kinds_except(parent, id);
        admit(parent_kind, child_kind, &siblings)?;
        if self.child_count_except(parent, id) + 1 > MAX_CHILDREN_PER_PARENT {
            return Err(Violation::TooManyChildren { parent });
        }
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
        let mut node = (*arc).clone();
        for (key, value) in props {
            if value.is_null() {
                node.raw.remove(&key);
            } else {
                node.raw.insert(key, value);
            }
        }
        if let Some(seq) = seq {
            node.seq = Some(seq);
        }
        node.element = rerealize(id, &node.element, &node.raw);
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
        self.nodes
            .iter()
            .find_map(|(parent, node)| node.children.contains(&id).then_some(*parent))
    }

    fn under(&self, ancestor: u32, start: u32) -> bool {
        let mut cursor = start;
        for _ in 0..MAX_NODES {
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

    fn child_kinds(&self, parent: u32) -> Vec<ElementKind> {
        self.nodes
            .get(&parent)
            .map(|node| {
                node.children
                    .iter()
                    .filter_map(|id| self.nodes.get(id).map(|child| child.element.kind()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn kinds_except(&self, parent: u32, except: u32) -> Vec<ElementKind> {
        self.nodes
            .get(&parent)
            .map(|node| {
                node.children
                    .iter()
                    .filter(|id| **id != except)
                    .filter_map(|id| self.nodes.get(id).map(|child| child.element.kind()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn child_count(&self, parent: u32) -> usize {
        self.nodes
            .get(&parent)
            .map(|node| node.children.len())
            .unwrap_or(0)
    }

    fn child_count_except(&self, parent: u32, except: u32) -> usize {
        self.nodes
            .get(&parent)
            .map(|node| node.children.iter().filter(|id| **id != except).count())
            .unwrap_or(0)
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
    }

    fn detach(&mut self, parent: u32, id: u32) {
        let Some(arc) = self.nodes.get(&parent).cloned() else {
            return;
        };
        let mut node = (*arc).clone();
        node.children.retain(|child| *child != id);
        self.nodes.insert(parent, Arc::new(node));
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
            Self::Entry(_) => ElementKind::Entry,
            Self::Placeholder(_) => ElementKind::Placeholder,
            Self::Box(_) => ElementKind::Box,
            Self::Label(_) => ElementKind::Label,
            Self::Image(_) => ElementKind::Image,
            Self::Button(_) => ElementKind::Button,
            Self::Switch(_) => ElementKind::Switch,
            Self::Scale(_) => ElementKind::Scale,
            Self::Spinner(_) => ElementKind::Spinner,
            Self::Progress(_) => ElementKind::Progress,
            Self::Separator(_) => ElementKind::Separator,
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
            Self::Entry => "entry",
            Self::Placeholder => "placeholder",
            Self::Box => "box",
            Self::Label => "label",
            Self::Image => "image",
            Self::Button => "button",
            Self::Switch => "switch",
            Self::Scale => "scale",
            Self::Spinner => "spinner",
            Self::Progress => "progress",
            Self::Separator => "separator",
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
        "entry" => ElementKind::Entry,
        "placeholder" => ElementKind::Placeholder,
        "box" => ElementKind::Box,
        "label" => ElementKind::Label,
        "image" => ElementKind::Image,
        "button" => ElementKind::Button,
        "switch" => ElementKind::Switch,
        "scale" => ElementKind::Scale,
        "spinner" => ElementKind::Spinner,
        "progress" => ElementKind::Progress,
        "separator" => ElementKind::Separator,
        _ => return None,
    })
}

fn admit(
    parent: ElementKind,
    child: ElementKind,
    siblings: &[ElementKind],
) -> Result<(), Violation> {
    if !kind_allowed(parent, child) {
        return Err(Violation::BadParent { parent, child });
    }
    let unique = matches!(
        (parent, child),
        (ElementKind::Root, ElementKind::Popover)
            | (ElementKind::Popover, ElementKind::Hero)
            | (ElementKind::Popover, ElementKind::Footer)
    );
    if unique && siblings.contains(&child) {
        return Err(Violation::AlreadyHolds { parent, child });
    }
    Ok(())
}

fn kind_allowed(parent: ElementKind, child: ElementKind) -> bool {
    match parent {
        ElementKind::Root => matches!(child, ElementKind::Indicator | ElementKind::Popover),
        ElementKind::Popover => {
            matches!(child, ElementKind::Hero | ElementKind::Footer) || is_body(child)
        }
        ElementKind::Section | ElementKind::Box => is_body(child) && child != ElementKind::Section,
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
            | ElementKind::Entry
            | ElementKind::Placeholder
            | ElementKind::Box
            | ElementKind::Label
            | ElementKind::Image
            | ElementKind::Button
            | ElementKind::Switch
            | ElementKind::Scale
            | ElementKind::Spinner
            | ElementKind::Progress
            | ElementKind::Separator
            | ElementKind::Unsupported
    )
}

fn compile(
    wire: WireNode,
    parent_kind: ElementKind,
    siblings: &[ElementKind],
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
    if children.len() > MAX_CHILDREN_PER_PARENT {
        return Err(Violation::TooManyChildren { parent: id });
    }
    let element_kind = kind_of(&kind).unwrap_or(ElementKind::Unsupported);
    admit(parent_kind, element_kind, siblings)?;
    let mut child_ids = Vec::with_capacity(children.len());
    let mut child_kinds = Vec::with_capacity(children.len());
    for child in children {
        let child_kind = kind_of(&child.kind).unwrap_or(ElementKind::Unsupported);
        let child_id = compile(child, element_kind, &child_kinds, existing, new_ids, built)?;
        child_ids.push(child_id);
        child_kinds.push(child_kind);
    }
    let element = realize_new(id, &kind, &props);
    built.push((
        id,
        Node {
            element,
            children: child_ids,
            seq: None,
            raw: props,
        },
    ));
    Ok(id)
}

fn realize_new(id: u32, type_name: &str, raw: &Map<String, Value>) -> Element {
    match kind_of(type_name) {
        Some(kind) => realize_kind(id, kind, raw),
        None => Element::Unsupported(glimpse_utils::clean(type_name, NAME_CAP)),
    }
}

fn rerealize(id: u32, previous: &Element, raw: &Map<String, Value>) -> Element {
    match previous.kind() {
        ElementKind::Unsupported | ElementKind::Root => previous.clone(),
        kind => realize_kind(id, kind, raw),
    }
}

fn realize_kind(id: u32, kind: ElementKind, raw: &Map<String, Value>) -> Element {
    match kind {
        ElementKind::Root => Element::Root,
        ElementKind::Popover => Element::Popover(read_props(id, raw)),
        ElementKind::Footer => Element::Footer(read_props(id, raw)),
        ElementKind::Indicator => Element::Indicator(finish_indicator(read_props(id, raw))),
        ElementKind::Hero => Element::Hero(finish_hero(read_props(id, raw))),
        ElementKind::Section => Element::Section(finish_section(read_props(id, raw))),
        ElementKind::Row => Element::Row(finish_row(read_props(id, raw))),
        ElementKind::SwitchRow => Element::SwitchRow(finish_switch_row(read_props(id, raw))),
        ElementKind::Fader => Element::Fader(finish_fader(read_props(id, raw))),
        ElementKind::Entry => Element::Entry(finish_entry(read_props(id, raw))),
        ElementKind::Placeholder => Element::Placeholder(finish_placeholder(read_props(id, raw))),
        ElementKind::Box => Element::Box(finish_box(read_props(id, raw))),
        ElementKind::Label => Element::Label(finish_label(read_props(id, raw))),
        ElementKind::Image => Element::Image(finish_image(read_props(id, raw))),
        ElementKind::Button => Element::Button(finish_button(read_props(id, raw))),
        ElementKind::Switch => Element::Switch(read_props(id, raw)),
        ElementKind::Scale => Element::Scale(finish_scale(read_props(id, raw))),
        ElementKind::Spinner => Element::Spinner(read_props(id, raw)),
        ElementKind::Progress => Element::Progress(finish_progress(read_props(id, raw))),
        ElementKind::Separator => Element::Separator(read_props(id, raw)),
        ElementKind::Unsupported => Element::Unsupported(String::new()),
    }
}

fn read_props<T: DeserializeOwned + Default>(id: u32, raw: &Map<String, Value>) -> T {
    let mut filtered = Map::new();
    for (key, value) in raw {
        if value.is_null() {
            continue;
        }
        if accepts::<T>(key, value) {
            filtered.insert(key.clone(), value.clone());
        } else {
            note_dropped(id, key);
        }
    }
    serde_json::from_value(Value::Object(filtered)).unwrap_or_default()
}

fn accepts<T: DeserializeOwned>(key: &str, value: &Value) -> bool {
    let mut trial = Map::new();
    trial.insert(key.to_owned(), value.clone());
    serde_json::from_value::<T>(Value::Object(trial)).is_ok()
}

fn note_dropped(id: u32, key: &str) {
    static DROPPED: LazyLock<Mutex<HashSet<(u32, String)>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));
    let fresh = DROPPED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert((id, key.to_owned()));
    if fresh {
        tracing::warn!(id, key, "dropped a property whose value has the wrong type");
    }
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

fn finish_hero(mut props: HeroProps) -> HeroProps {
    props.icon = icon(props.icon);
    props.title = text(props.title);
    props.subtitle = text(props.subtitle);
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

fn text(value: Option<String>) -> Option<String> {
    capped(value, TEXT_CAP)
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

fn icon(value: Option<String>) -> Option<String> {
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
mod tests {
    use std::sync::Arc;

    use serde::de::IntoDeserializer;
    use serde_json::{Map, Value, json};

    use super::{
        Element, ElementKind, EntryProps, FaderProps, IndicatorProps, LabelProps, ROOT, RowProps,
        ScaleProps, Severity, Tree, Violation, de_finite_f64,
    };
    use crate::services::exec::wire::{FromApplet, Op, WireNode};

    const HELLO: &str = include_str!("../../../../../sdk/applet/fixtures/hello.ndjson");
    const OPS: &str = include_str!("../../../../../sdk/applet/fixtures/ops.ndjson");
    const UNKNOWN: &str =
        include_str!("../../../../../sdk/applet/fixtures/invalid/unknown-id.ndjson");
    const CYCLE: &str = include_str!("../../../../../sdk/applet/fixtures/invalid/cycle.ndjson");
    const BAD_PARENT: &str =
        include_str!("../../../../../sdk/applet/fixtures/invalid/bad-parent.ndjson");
    const TOO_MANY_NODES: &str =
        include_str!("../../../../../sdk/applet/fixtures/invalid/max-nodes.ndjson");
    const TOO_MANY_CHILDREN: &str =
        include_str!("../../../../../sdk/applet/fixtures/invalid/max-children.ndjson");
    const GARBAGE: &str = include_str!("../../../../../sdk/applet/fixtures/invalid/garbage.ndjson");

    fn lines(source: &str) -> impl Iterator<Item = &str> {
        source
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    }

    fn commits(source: &str) -> Vec<Vec<Op>> {
        lines(source)
            .map(
                |line| match serde_json::from_str::<FromApplet>(line).expect("commit") {
                    FromApplet::Commit { ops } => ops,
                    other => panic!("expected a commit, got {other:?}"),
                },
            )
            .collect()
    }

    fn props(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    }

    fn wire(id: u32, kind: &str, value: Value, children: Vec<WireNode>) -> WireNode {
        WireNode {
            id,
            kind: kind.to_owned(),
            props: props(value),
            children,
        }
    }

    fn leaf(id: u32, kind: &str) -> WireNode {
        wire(id, kind, json!({}), Vec::new())
    }

    fn insert(parent: u32, node: WireNode, before: Option<u32>) -> Op {
        Op::Insert {
            parent,
            node,
            before,
        }
    }

    fn row_of(element: &Element) -> &RowProps {
        match element {
            Element::Row(props) => props,
            other => panic!("expected a row, got {other:?}"),
        }
    }

    fn indicator_of(element: &Element) -> &IndicatorProps {
        match element {
            Element::Indicator(props) => props,
            other => panic!("expected an indicator, got {other:?}"),
        }
    }

    fn label_of(element: &Element) -> &LabelProps {
        match element {
            Element::Label(props) => props,
            other => panic!("expected a label, got {other:?}"),
        }
    }

    fn fader_of(element: &Element) -> &FaderProps {
        match element {
            Element::Fader(props) => props,
            other => panic!("expected a fader, got {other:?}"),
        }
    }

    fn entry_of(element: &Element) -> &EntryProps {
        match element {
            Element::Entry(props) => props,
            other => panic!("expected an entry, got {other:?}"),
        }
    }

    fn hosted(tree: &Tree) -> Tree {
        tree.apply(vec![insert(ROOT, leaf(1, "popover"), None)])
            .expect("popover")
    }

    #[test]
    fn text_is_cleaned_icons_dropped_and_unsupported_names_are_cleaned() {
        let hostile = "<b>hi</b>\u{7}\u{202e}there";
        let long = "a".repeat(10_000);
        let type_name = "<b>x</b>\u{202e}";
        let tree = hosted(&Tree::default())
            .apply(vec![insert(
                1,
                wire(
                    2,
                    "row",
                    json!({"title": hostile, "icon": "/etc/passwd", "value": "<b>x</b>"}),
                    Vec::new(),
                ),
                None,
            )])
            .expect("row")
            .apply(vec![insert(
                1,
                wire(3, "label", json!({"text": long}), Vec::new()),
                None,
            )])
            .expect("label")
            .apply(vec![insert(1, leaf(4, type_name), None)])
            .expect("unsupported");

        let row = row_of(&tree.node(2).expect("row").element);
        assert_eq!(
            row.title.as_deref(),
            Some(glimpse_utils::clean(hostile, 256).as_str())
        );
        assert!(
            row.title
                .as_ref()
                .is_some_and(|title| title.contains("<b>hi</b>"))
        );
        assert_eq!(row.icon, None);
        assert_eq!(row.value.as_deref(), Some("<b>x</b>"));
        assert_eq!(
            label_of(&tree.node(3).expect("label").element)
                .text
                .as_deref(),
            Some(glimpse_utils::clean(&long, 256).as_str())
        );
        assert_eq!(
            tree.node(4).expect("unsupported").element,
            Element::Unsupported(glimpse_utils::clean(type_name, 64))
        );
    }

    #[test]
    fn entry_value_keeps_spaces_and_strips_controls() {
        let tree = hosted(&Tree::default())
            .apply(vec![insert(
                1,
                wire(
                    2,
                    "entry",
                    json!({"value": "  hello \u{7}\u{202e}"}),
                    Vec::new(),
                ),
                None,
            )])
            .expect("entry");
        assert_eq!(
            entry_of(&tree.node(2).expect("entry").element).value,
            "  hello "
        );

        let long = format!("{} ", "a".repeat(4096));
        let tree = tree
            .apply(vec![Op::Set {
                id: 2,
                props: props(json!({"value": long})),
                seq: None,
            }])
            .expect("cap");
        let value = &entry_of(&tree.node(2).expect("entry").element).value;
        assert_eq!(value.chars().count(), 4096);
        assert!(value.ends_with('a'));
    }

    #[test]
    fn a_nan_fader_value_is_dropped() {
        let nan: serde::de::value::F64Deserializer<serde::de::value::Error> =
            f64::NAN.into_deserializer();
        assert!(de_finite_f64(nan).is_err());
        let infinite: serde::de::value::F64Deserializer<serde::de::value::Error> =
            f64::INFINITY.into_deserializer();
        assert!(de_finite_f64(infinite).is_err());

        let tree = hosted(&Tree::default())
            .apply(vec![insert(
                1,
                wire(
                    2,
                    "fader",
                    json!({"value": "NaN", "maximum": 40, "icon": "audio-volume-high-symbolic"}),
                    Vec::new(),
                ),
                None,
            )])
            .expect("fader");
        let fader = fader_of(&tree.node(2).expect("fader").element);
        assert_eq!(fader.value, 0.0);
        assert_eq!(fader.maximum, 40.0);
        assert_eq!(fader.floor, 0.0);
        assert_eq!(fader.icon.as_deref(), Some("audio-volume-high-symbolic"));

        let bare = tree
            .apply(vec![insert(
                1,
                wire(3, "fader", json!({}), Vec::new()),
                None,
            )])
            .expect("defaults");
        let fader = fader_of(&bare.node(3).expect("fader").element);
        assert_eq!(fader.value, 0.0);
        assert_eq!(fader.maximum, 100.0);
        assert_eq!(fader.floor, 0.0);
    }

    #[test]
    fn indicator_props_decode_badge_cap_and_bad_icons() {
        let tree = Tree::default()
            .apply(vec![insert(
                ROOT,
                wire(
                    1,
                    "indicator",
                    json!({
                        "icon": "audio-volume-high-symbolic",
                        "overlay": "emblem-important-symbolic",
                        "dot": "#e01b24",
                        "badge": "123456789",
                        "severity": "warning",
                        "attention": true,
                        "notice": true,
                        "className": "accent",
                        "onPress": true,
                        "text": "2"
                    }),
                    Vec::new(),
                ),
                None,
            )])
            .expect("indicator");
        let props = indicator_of(&tree.node(1).expect("indicator").element);
        assert_eq!(props.icon.as_deref(), Some("audio-volume-high-symbolic"));
        assert_eq!(props.overlay.as_deref(), Some("emblem-important-symbolic"));
        assert_eq!(props.dot.as_deref(), Some("#e01b24"));
        assert_eq!(
            props.badge.as_deref(),
            Some(glimpse_utils::clean("123456789", 8).as_str())
        );
        assert_eq!(props.severity, Some(Severity::Warning));
        assert!(props.attention);
        assert!(props.notice);
        assert_eq!(props.class_name, Some(super::ClassName::Accent));
        assert!(props.on_press);
        assert_eq!(props.text.as_deref(), Some("2"));

        let tree = Tree::default()
            .apply(vec![insert(
                ROOT,
                wire(
                    1,
                    "indicator",
                    json!({
                        "icon": "/etc/passwd",
                        "overlay": "bad icon",
                        "badge": "3",
                        "className": "nope",
                        "text": "ok"
                    }),
                    Vec::new(),
                ),
                None,
            )])
            .expect("dropped");
        let props = indicator_of(&tree.node(1).expect("indicator").element);
        assert_eq!(props.icon, None);
        assert_eq!(props.overlay, None);
        assert_eq!(props.class_name, None);
        assert_eq!(props.badge.as_deref(), Some("3"));
        assert_eq!(props.text.as_deref(), Some("ok"));
    }

    #[test]
    fn set_keeps_other_arcs_deletes_null_and_drops_wrong_types() {
        let tree = hosted(&Tree::default())
            .apply(vec![
                insert(
                    1,
                    wire(2, "row", json!({"title": "Milk"}), Vec::new()),
                    None,
                ),
                insert(
                    1,
                    wire(3, "row", json!({"title": "Bread"}), Vec::new()),
                    None,
                ),
            ])
            .expect("rows");
        let kept = Arc::clone(tree.node(3).expect("other"));
        let updated = tree
            .apply(vec![Op::Set {
                id: 2,
                props: props(json!({"title": "Oat", "busy": "nope", "subtitle": "later"})),
                seq: None,
            }])
            .expect("set");
        assert!(Arc::ptr_eq(&kept, updated.node(3).expect("other")));
        assert!(!Arc::ptr_eq(
            tree.node(2).expect("before"),
            updated.node(2).expect("after")
        ));
        assert!(Arc::ptr_eq(
            tree.node(1).expect("parent"),
            updated.node(1).expect("parent")
        ));
        let row = row_of(&updated.node(2).expect("row").element);
        assert_eq!(row.title.as_deref(), Some("Oat"));
        assert_eq!(row.subtitle.as_deref(), Some("later"));
        assert!(!row.busy);
        assert_eq!(
            updated.node(2).expect("row").raw.get("busy"),
            Some(&json!("nope"))
        );

        let cleared = updated
            .apply(vec![Op::Set {
                id: 2,
                props: props(json!({"title": null})),
                seq: Some(4),
            }])
            .expect("null");
        assert!(Arc::ptr_eq(&kept, cleared.node(3).expect("other")));
        let node = cleared.node(2).expect("row");
        assert!(!node.raw.contains_key("title"));
        assert_eq!(row_of(&node.element).title, None);
        assert_eq!(row_of(&node.element).subtitle.as_deref(), Some("later"));
        assert_eq!(node.seq, Some(4));
    }

    #[test]
    fn move_reorders_remove_drops_subtree_insert_honors_before() {
        let mut tree = Tree::default();
        for ops in commits(OPS) {
            tree = tree.apply(ops).expect("ops fixture");
        }
        assert_eq!(tree.children(ROOT), &[1]);
        assert_eq!(tree.children(1), &[2]);
        assert_eq!(tree.children(2), &[5, 3, 6, 4]);
        assert!(tree.node(7).is_none());
        assert!(tree.node(8).is_none());
        assert!(tree.node(9).is_none());
        let row = cleared_row(&tree);
        assert_eq!(row.title, None);
        assert!(!row.busy);
        assert_eq!(tree.node(3).expect("row").seq, None);
        assert_eq!(
            row_of(&tree.node(5).expect("row").element).title.as_deref(),
            Some("c")
        );
    }

    fn cleared_row(tree: &Tree) -> &RowProps {
        row_of(&tree.node(3).expect("row").element)
    }

    #[test]
    fn hello_fixture_builds_the_first_tree() {
        let mut tree = Tree::default();
        for line in lines(HELLO) {
            if let FromApplet::Commit { ops } = serde_json::from_str(line).expect("line") {
                tree = tree.apply(ops).expect("hello commit");
            }
        }
        assert_eq!(tree.children(ROOT), &[1, 2]);
        assert_eq!(tree.children(2), &[3, 4]);
        assert_eq!(tree.children(4), &[5, 6]);
        assert_eq!(
            indicator_of(&tree.node(1).expect("chip").element)
                .text
                .as_deref(),
            Some("2")
        );
        match &tree.node(3).expect("hero").element {
            Element::Hero(hero) => assert_eq!(hero.title.as_deref(), Some("Todos")),
            other => panic!("expected a hero, got {other:?}"),
        }
        assert!(row_of(&tree.node(6).expect("row").element).on_activate);
    }

    #[test]
    fn invalid_batches_leave_the_tree_unchanged() {
        let tree = Tree::default();
        let root = Arc::clone(tree.node(ROOT).expect("root"));
        let cases = [
            (UNKNOWN, Violation::UnknownId(9)),
            (CYCLE, Violation::Cycle { id: 2, parent: 3 }),
            (
                BAD_PARENT,
                Violation::BadParent {
                    parent: ElementKind::Indicator,
                    child: ElementKind::Row,
                },
            ),
            (TOO_MANY_NODES, Violation::TooManyNodes),
            (TOO_MANY_CHILDREN, Violation::TooManyChildren { parent: 2 }),
        ];
        for (source, expected) in cases {
            let mut batches = commits(source);
            let ops = batches.remove(0);
            let error = tree.apply(ops).expect_err("violation");
            assert_eq!(error, expected);
            assert!(Arc::ptr_eq(&root, tree.node(ROOT).expect("root")));
            assert!(tree.children(ROOT).is_empty());
        }
        let line = lines(GARBAGE).next().expect("garbage");
        assert!(serde_json::from_str::<FromApplet>(line).is_err());
    }

    #[test]
    fn the_parent_table_rejects_the_combinations_it_names() {
        let tree = hosted(&Tree::default());
        let section = tree
            .apply(vec![insert(
                1,
                wire(2, "section", json!({}), vec![leaf(3, "row")]),
                None,
            )])
            .expect("section");
        assert_eq!(
            section
                .apply(vec![insert(2, leaf(4, "section"), None)])
                .expect_err("nested section"),
            Violation::BadParent {
                parent: ElementKind::Section,
                child: ElementKind::Section,
            }
        );
        let boxed = tree
            .apply(vec![insert(1, leaf(2, "box"), None)])
            .expect("box");
        assert_eq!(
            boxed
                .apply(vec![insert(2, leaf(3, "section"), None)])
                .expect_err("section in a box"),
            Violation::BadParent {
                parent: ElementKind::Box,
                child: ElementKind::Section,
            }
        );
        assert_eq!(
            tree.apply(vec![insert(ROOT, leaf(2, "popover"), None)])
                .expect_err("second popover"),
            Violation::AlreadyHolds {
                parent: ElementKind::Root,
                child: ElementKind::Popover,
            }
        );
        let hero = tree
            .apply(vec![insert(1, leaf(2, "hero"), None)])
            .expect("hero");
        assert_eq!(
            hero.apply(vec![insert(1, leaf(3, "hero"), None)])
                .expect_err("second hero"),
            Violation::AlreadyHolds {
                parent: ElementKind::Popover,
                child: ElementKind::Hero,
            }
        );
        let footer = tree
            .apply(vec![insert(
                1,
                wire(2, "footer", json!({}), vec![leaf(3, "button")]),
                None,
            )])
            .expect("footer");
        match &footer.node(3).expect("button").element {
            Element::Button(button) => assert!(button.sensitive),
            other => panic!("expected a button, got {other:?}"),
        }
        assert_eq!(
            footer
                .apply(vec![insert(2, leaf(4, "section"), None)])
                .expect_err("section in a footer"),
            Violation::BadParent {
                parent: ElementKind::Footer,
                child: ElementKind::Section,
            }
        );
        assert_eq!(
            section
                .apply(vec![insert(3, leaf(4, "label"), None)])
                .expect_err("child of a leaf"),
            Violation::BadParent {
                parent: ElementKind::Row,
                child: ElementKind::Label,
            }
        );
    }

    #[test]
    fn two_hundred_children_fit_and_one_more_does_not() {
        let children = (3..=202).map(|id| leaf(id, "label")).collect();
        let tree = hosted(&Tree::default())
            .apply(vec![insert(1, wire(2, "box", json!({}), children), None)])
            .expect("200 labels");
        assert_eq!(tree.children(2).len(), 200);
        assert_eq!(
            tree.apply(vec![insert(2, leaf(203, "label"), None)])
                .expect_err("201st"),
            Violation::TooManyChildren { parent: 2 }
        );
    }

    fn walk(tree: &Tree, id: u32) -> usize {
        1 + tree
            .children(id)
            .iter()
            .map(|child| walk(tree, *child))
            .sum::<usize>()
    }

    #[test]
    fn the_node_cap_counts_the_root() {
        let mut next = 2;
        let mut ops = Vec::new();
        let mut last_box = 0;
        for index in 0..2 {
            let box_id = next;
            next += 1;
            last_box = box_id;
            let count = if index == 0 { 200 } else { 96 };
            let mut children = Vec::with_capacity(count);
            for _ in 0..count {
                children.push(leaf(next, "label"));
                next += 1;
            }
            ops.push(insert(1, wire(box_id, "box", json!({}), children), None));
        }
        let tree = hosted(&Tree::default()).apply(ops).expect("300 nodes");
        assert_eq!(walk(&tree, ROOT), super::MAX_NODES);
        assert_eq!(
            tree.apply(vec![insert(last_box, leaf(next, "label"), None)])
                .expect_err("one past the cap"),
            Violation::TooManyNodes
        );
    }

    #[test]
    fn scale_range_resets_when_min_is_not_below_max() {
        let tree = hosted(&Tree::default())
            .apply(vec![insert(
                1,
                wire(
                    2,
                    "scale",
                    json!({"min": 5, "max": 5, "value": 9}),
                    Vec::new(),
                ),
                None,
            )])
            .expect("scale");
        match &tree.node(2).expect("scale").element {
            Element::Scale(ScaleProps {
                min, max, value, ..
            }) => {
                assert_eq!((*min, *max, *value), (0.0, 1.0, 1.0));
            }
            other => panic!("expected a scale, got {other:?}"),
        }
    }

    #[test]
    fn a_failed_batch_does_not_keep_an_earlier_op() {
        let tree = hosted(&Tree::default());
        let error = tree
            .apply(vec![
                insert(1, leaf(2, "row"), None),
                insert(2, leaf(3, "label"), None),
            ])
            .expect_err("leaf");
        assert_eq!(
            error,
            Violation::BadParent {
                parent: ElementKind::Row,
                child: ElementKind::Label,
            }
        );
        assert!(tree.node(2).is_none());
    }
}
