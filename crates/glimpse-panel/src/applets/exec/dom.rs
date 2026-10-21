use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

use gettextrs::gettext;
use glimpse_services::{
    CalendarProps, ClassName, Element, ElementKind, ExecUserEvent as UserEvent, Node, Paint,
    PopoverSize as ExecSize, ROOT, RowTone as ExecRowTone, RowTypography as ExecTypography,
    SwatchSize, ToggleControl as ExecControl, Tone, Transition, Tree,
};
use glimpse_widgets::{
    ActionRow, Bars, Calendar, ChoiceList, Container, ContainerPaint, DOTS_SIZE, Dots, EntryRow,
    ExpanderRow, MeterRow, Notice, Placeholder, PopoverShell, PopoverSize, RangeBar, Readout,
    Row as UiRow, RowTone, RowTypography, Section, SliderRow, Sparkline, Swatch, ToggleControl,
    ToggleRow, Ymd, reconcile::by_key,
};
use gtk4::{gdk, glib, prelude::*};
use serde_json::{Number, Value};

use super::render;
use crate::applet::Opener;

pub(super) type Key = (u64, u32, ElementKind);
type Dispatch = Rc<dyn Fn(UserEvent)>;

const DIM_ALPHA: f32 = 0.55;
const MARKER_LIMIT: usize = 3;
const OPTION_CHARS: i32 = 32;

macro_rules! dress_static_row {
    ($widget:expr, $props:expr) => {{
        let p = $props;
        $widget.set_property("icon-name", p.icon.as_deref());
        $widget.set_property("title", p.title.as_deref());
        $widget.set_property("subtitle", p.subtitle.as_deref());
        $widget.set_property("value", p.value.as_deref());
        $widget.set_property("tone", row_tone(p.tone));
        $widget.set_property("inactive", p.inactive);
        $widget.set_property("tooltip", p.tooltip.as_deref());
        $widget.set_property("typography", row_typography(p.typography));
    }};
}

macro_rules! dress_row_base {
    ($widget:expr, $props:expr) => {{
        let p = $props;
        dress_static_row!($widget, p);
        $widget.set_property("busy", p.busy);
    }};
}

struct Mounted {
    widget: gtk4::Widget,
    inner: Option<gtk4::Box>,
    node: Arc<Node>,
    handlers: Vec<glib::SignalHandlerId>,
}

pub struct Dom {
    shell: glib::WeakRef<PopoverShell>,
    closed: Rc<Cell<bool>>,
    popover_class: Option<&'static str>,
    body: gtk4::Box,
    nodes: HashMap<Key, Mounted>,
    held: HashMap<Key, Vec<(Key, gtk4::Widget)>>,
    seq: Rc<RefCell<HashMap<Key, u64>>>,
    dispatch: Dispatch,
    slot: u64,
    opener: Opener,
    generation: Option<u64>,
    applying: Rc<Cell<bool>>,
    tones_dirty: Rc<Cell<bool>>,
    style_manager: adw::StyleManager,
    style_handlers: Vec<glib::SignalHandlerId>,
}

impl Dom {
    pub fn new(
        shell: &PopoverShell,
        slot: u64,
        dispatch: Dispatch,
        opener: Opener,
        seq: Rc<RefCell<HashMap<Key, u64>>>,
    ) -> Self {
        let body = shell.body();
        body.set_visible(false);
        let closed = Rc::new(Cell::new(false));
        shell.connect_unmap({
            let closed = closed.clone();
            move |_| closed.set(true)
        });
        let tones_dirty = Rc::new(Cell::new(false));
        let style_manager = adw::StyleManager::default();
        let style_handlers = ["dark", "accent-color"]
            .into_iter()
            .map(|property| {
                let tones_dirty = tones_dirty.clone();
                let opener = opener.clone();
                style_manager.connect_notify_local(Some(property), move |_, _| {
                    tones_dirty.set(true);
                    opener.wake();
                })
            })
            .collect();
        Self {
            shell: shell.downgrade(),
            closed,
            popover_class: None,
            body,
            nodes: HashMap::new(),
            held: HashMap::new(),
            seq,
            dispatch,
            slot,
            opener,
            generation: None,
            applying: Rc::new(Cell::new(false)),
            tones_dirty,
            style_manager,
            style_handlers,
        }
    }

    pub fn is_alive(&self) -> bool {
        !self.closed.get() && self.shell.upgrade().is_some()
    }

    pub fn needs_redress(&self) -> bool {
        self.tones_dirty.get()
    }

    pub fn reconcile(&mut self, tree: &Tree, generation: u64) -> bool {
        self.applying.set(true);
        let alive = self.reconcile_tree(tree, generation);
        self.tones_dirty.set(false);
        self.applying.set(false);
        if alive {
            self.report_closed_expanders();
        }
        alive
    }

    fn report_closed_expanders(&self) {
        let drifted: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|(key, mounted)| match &mounted.node.element {
                Element::ExpanderRow(props) if props.on_toggle => {
                    let row = mounted.widget.downcast_ref::<ExpanderRow>()?;
                    (row.open() != props.open).then_some((*key, row.open()))
                }
                _ => None,
            })
            .collect();
        for (key, expanded) in drifted {
            emit(
                &self.dispatch,
                self.slot,
                key,
                "onToggle",
                vec![Value::Bool(expanded)],
                Some(bump(&self.seq, key)),
            );
        }
    }

    fn reconcile_tree(&mut self, tree: &Tree, generation: u64) -> bool {
        let Some(shell) = self.shell.upgrade() else {
            return false;
        };
        if self.generation != Some(generation) {
            self.clear();
            self.generation = Some(generation);
        }
        let mut live = HashSet::new();
        let Some(popover) =
            render::first_child_of_kind(tree, 0, |element| matches!(element, Element::Popover(_)))
        else {
            self.clear();
            return true;
        };
        let children = tree.children(popover);
        let popover_class = tree
            .node(popover)
            .and_then(|node| element_class(&node.element))
            .map(render::class_name);
        swap_class(shell.upcast_ref(), &mut self.popover_class, popover_class);
        if let Some(node) = tree.node(popover)
            && let Element::Popover(props) = &node.element
        {
            shell.set_icon_name(props.icon.as_deref());
            shell.set_title(props.title.as_deref());
            shell.set_subtitle(props.subtitle.as_deref());
            shell.set_size(match props.size {
                Some(ExecSize::Wide) => PopoverSize::Wide,
                _ => PopoverSize::Regular,
            });
        }
        self.mount_addon(&shell, tree, generation, popover, &mut live);
        let root_key = (generation, ROOT, ElementKind::Popover);
        let body_ids: Vec<_> = children.iter().copied().filter(|id| matches!(tree.node(*id).map(|node| &node.element), Some(element) if !matches!(element, Element::Addon(_)))).collect();
        self.reconcile_children(
            tree,
            generation,
            root_key,
            &self.body.clone(),
            &body_ids,
            &mut live,
        );
        self.body.set_visible(self.body.first_child().is_some());
        self.opener.typing(live.iter().any(|key| {
            matches!(
                key.2,
                ElementKind::Entry | ElementKind::EntryRow | ElementKind::Scale
            )
        }));
        self.nodes.retain(|key, _| live.contains(key));
        self.held
            .retain(|key, _| key.0 == generation && (live.contains(key) || *key == root_key));
        true
    }

    fn mount_addon(
        &mut self,
        shell: &PopoverShell,
        tree: &Tree,
        generation: u64,
        popover: u32,
        live: &mut HashSet<Key>,
    ) {
        let Some(addon) = render::first_child_of_kind(tree, popover, |element| {
            matches!(element, Element::Addon(_))
        }) else {
            shell.set_addon(None::<&gtk4::Widget>);
            return;
        };
        let Some(child) = tree.children(addon).first().copied() else {
            shell.set_addon(None::<&gtk4::Widget>);
            return;
        };
        match self.mount(tree, generation, child, live) {
            Some((_, widget)) => shell.set_addon(Some(&widget)),
            None => shell.set_addon(None::<&gtk4::Widget>),
        }
    }

    fn clear(&mut self) {
        for mounted in self.nodes.values() {
            if mounted.widget.parent().as_ref() == Some(self.body.upcast_ref()) {
                self.body.remove(&mounted.widget);
            }
        }
        self.body.set_visible(self.body.first_child().is_some());
        self.nodes.clear();
        self.held.clear();
        if let Some(shell) = self.shell.upgrade() {
            swap_class(shell.upcast_ref(), &mut self.popover_class, None);
            shell.set_icon_name(None::<&str>);
            shell.set_title(None::<&str>);
            shell.set_subtitle(None::<&str>);
            shell.set_size(PopoverSize::Regular);
            shell.set_addon(None::<&gtk4::Widget>);
        }
        self.opener.typing(false);
    }

    fn reconcile_children(
        &mut self,
        tree: &Tree,
        generation: u64,
        parent: Key,
        inner: &gtk4::Box,
        ids: &[u32],
        live: &mut HashSet<Key>,
    ) {
        let wanted: Vec<(Key, gtk4::Widget)> = ids
            .iter()
            .filter_map(|id| self.mount(tree, generation, *id, live))
            .collect();
        let stacks = matches!(
            parent.2,
            ElementKind::Section
                | ElementKind::Popover
                | ElementKind::Revealer
                | ElementKind::Container
        );
        for (key, widget) in &wanted {
            match stacks && is_bare(key.2) {
                true => widget.add_css_class("exec-bare"),
                false => widget.remove_css_class("exec-bare"),
            }
        }
        let held = self.held.entry(parent).or_default();
        held.retain(|(key, widget)| {
            !wanted
                .iter()
                .any(|(next_key, next_widget)| key == next_key && widget != next_widget)
        });
        by_key(
            inner,
            held,
            &wanted,
            |(key, _)| *key,
            |(_, widget)| widget.clone(),
            |_, _| {},
        );
        inner.set_visible(!wanted.is_empty());
    }

    fn mount(
        &mut self,
        tree: &Tree,
        generation: u64,
        id: u32,
        live: &mut HashSet<Key>,
    ) -> Option<(Key, gtk4::Widget)> {
        let node = tree.node(id)?.clone();
        let key = (generation, id, node.element.kind());
        live.insert(key);
        if !self.nodes.contains_key(&key) {
            let (widget, inner, handlers) = self.build(&node, key);
            self.nodes.insert(
                key,
                Mounted {
                    widget,
                    inner,
                    node: node.clone(),
                    handlers,
                },
            );
            self.dress(key, &node, true);
        } else {
            self.dress(key, &node, false);
        }
        let mounted = self.nodes.get(&key)?;
        let widget = mounted.widget.clone();
        let inner = mounted.inner.clone();
        if let Some(inner) = inner {
            let children: Vec<_> = node
                .children
                .iter()
                .copied()
                .filter(|id| {
                    tree.node(*id).is_some_and(|node| {
                        !matches!(node.element, Element::End(_) | Element::Start(_))
                    })
                })
                .collect();
            self.reconcile_children(tree, generation, key, &inner, &children, live);
            let end =
                render::first_child_of_kind(tree, id, |element| matches!(element, Element::End(_)))
                    .and_then(|id| self.mount(tree, generation, id, live))
                    .map(|(_, widget)| widget)
                    .filter(|widget| widget.first_child().is_some());
            let start = render::first_child_of_kind(tree, id, |element| {
                matches!(element, Element::Start(_))
            })
            .and_then(|id| self.mount(tree, generation, id, live))
            .map(|(_, widget)| widget);
            let lead = start
                .clone()
                .filter(|widget| widget.first_child().is_some());
            if let Some(row) = widget.downcast_ref::<ExpanderRow>() {
                match end {
                    Some(end) => row.set_end(&end),
                    None => row.clear_end(),
                }
                match lead {
                    Some(lead) => row.set_start(&lead),
                    None => row.clear_start(),
                }
            } else if let Some(row) = widget.downcast_ref::<SliderRow>() {
                let start = start.unwrap_or_else(|| inner.clone().upcast());
                row.set_start(&start);
                if let Some(parent) = start.parent() {
                    parent.set_visible(start.first_child().is_some());
                }
            } else if let Some(row) = widget.downcast_ref::<EntryRow>() {
                match inner.first_child() {
                    Some(_) => row.set_end(&inner),
                    None => row.clear_end(),
                }
            } else if matches!(
                &node.element,
                Element::Row(_) | Element::ActionRow(_) | Element::MeterRow(_)
            ) && let Some(row) = widget.downcast_ref::<UiRow>()
            {
                match inner.first_child() {
                    Some(_) => row.set_end(&inner),
                    None => row.clear_end(),
                }
                match lead {
                    Some(lead) => row.set_start(&lead),
                    None => row.clear_start(),
                }
            }
        }
        if let Element::Stack(props) = &node.element {
            self.reconcile_stack(tree, generation, key, props.visible, &node.children, live);
        }
        Some((key, widget))
    }

    fn reconcile_stack(
        &mut self,
        tree: &Tree,
        generation: u64,
        key: Key,
        visible: u32,
        ids: &[u32],
        live: &mut HashSet<Key>,
    ) {
        let wanted: Vec<(Key, gtk4::Widget)> = ids
            .iter()
            .filter_map(|id| self.mount(tree, generation, *id, live))
            .collect();
        let Some(stack) = self
            .nodes
            .get(&key)
            .and_then(|mounted| mounted.widget.downcast_ref::<gtk4::Stack>().cloned())
        else {
            return;
        };
        let held = self.held.entry(key).or_default();
        if *held != wanted {
            for (_, widget) in held.drain(..) {
                if widget.parent().as_ref() == Some(stack.upcast_ref()) {
                    stack.remove(&widget);
                }
            }
            for (index, (_, widget)) in wanted.iter().enumerate() {
                if widget.parent().is_some() {
                    widget.unparent();
                }
                stack.add_named(widget, Some(&index.to_string()));
            }
            *held = wanted;
        }
        let name = visible.to_string();
        if stack.child_by_name(&name).is_some()
            && stack.visible_child_name().as_deref() != Some(name.as_str())
        {
            stack.set_visible_child_name(&name);
        }
    }

    fn build(
        &self,
        node: &Node,
        key: Key,
    ) -> (gtk4::Widget, Option<gtk4::Box>, Vec<glib::SignalHandlerId>) {
        let mut handlers = Vec::new();
        let mut inner = None;
        let widget: gtk4::Widget = match &node.element {
            Element::Section(_) => {
                let section = Section::new();
                let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                section.set_content(Some(&content));
                inner = Some(content);
                section.upcast()
            }
            Element::Box(_) => {
                let box_ = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                inner = Some(box_.clone());
                box_.upcast()
            }
            Element::ActionRow(_) => {
                let row = ActionRow::new();
                inner = Some(gtk4::Box::new(gtk4::Orientation::Horizontal, 0));
                let dispatch = self.dispatch.clone();
                let slot = self.slot;
                handlers.push(row.connect_activated(move |_| {
                    emit(&dispatch, slot, key, "onActivate", vec![], None)
                }));
                row.upcast()
            }
            Element::ToggleRow(_) => {
                let row = ToggleRow::new();
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(row.connect_toggled(move |_, active| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onToggle",
                        vec![Value::Bool(active)],
                        Some(bump(&seq, key)),
                    )
                }));
                row.upcast()
            }
            Element::Row(_) => {
                inner = Some(gtk4::Box::new(gtk4::Orientation::Horizontal, 0));
                UiRow::new().upcast()
            }
            Element::MeterRow(_) => {
                inner = Some(gtk4::Box::new(gtk4::Orientation::Horizontal, 0));
                MeterRow::new().upcast()
            }
            Element::ExpanderRow(_) => {
                let row = ExpanderRow::new();
                inner = Some(row.details());
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let applying = self.applying.clone();
                let slot = self.slot;
                handlers.push(row.connect_toggled(move |_, open| {
                    if !applying.get() {
                        emit(
                            &dispatch,
                            slot,
                            key,
                            "onToggle",
                            vec![Value::Bool(open)],
                            Some(bump(&seq, key)),
                        );
                    }
                }));
                row.upcast()
            }
            Element::End(_) | Element::Start(_) => {
                let end = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                inner = Some(end.clone());
                end.upcast()
            }
            Element::SliderRow(_) => {
                let row = SliderRow::new();
                inner = Some(gtk4::Box::new(gtk4::Orientation::Horizontal, 0));
                let slot = self.slot;
                let dispatch = self.dispatch.clone();
                handlers.push(row.connect_moved(move |_, value| {
                    if let Some(value) = Number::from_f64(value) {
                        emit(
                            &dispatch,
                            slot,
                            key,
                            "onMove",
                            vec![Value::Number(value)],
                            None,
                        );
                    }
                }));
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                handlers.push(row.connect_changed(move |_, value| {
                    if let Some(value) = Number::from_f64(value) {
                        emit(
                            &dispatch,
                            slot,
                            key,
                            "onChange",
                            vec![Value::Number(value)],
                            Some(bump(&seq, key)),
                        );
                    }
                }));
                row.upcast()
            }
            Element::Entry(_) => {
                let entry = gtk4::Entry::new();
                entry.set_max_length(4096);
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(entry.connect_changed(move |entry| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onChange",
                        vec![Value::String(entry.text().to_string())],
                        Some(bump(&seq, key)),
                    )
                }));
                let dispatch = self.dispatch.clone();
                handlers.push(entry.connect_activate(move |entry| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onSubmit",
                        vec![Value::String(entry.text().to_string())],
                        None,
                    )
                }));
                entry.upcast()
            }
            Element::EntryRow(_) => {
                let row = EntryRow::new();
                inner = Some(gtk4::Box::new(gtk4::Orientation::Horizontal, 0));
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(row.connect_changed(move |row| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onChange",
                        vec![Value::String(row.text())],
                        Some(bump(&seq, key)),
                    )
                }));
                let dispatch = self.dispatch.clone();
                handlers.push(row.connect_activated(move |row| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onSubmit",
                        vec![Value::String(row.text())],
                        None,
                    )
                }));
                row.upcast()
            }
            Element::FactList(_) => gtk4::Box::new(gtk4::Orientation::Vertical, 0).upcast(),
            Element::ChoiceList(_) => {
                let list = ChoiceList::new();
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(list.connect_activated(move |_, index| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onSelect",
                        vec![Value::from(index)],
                        Some(bump(&seq, key)),
                    )
                }));
                list.upcast()
            }
            Element::Readout(_) => Readout::new().upcast(),
            Element::Notice(_) => {
                let notice = Notice::new();
                let dispatch = self.dispatch.clone();
                let slot = self.slot;
                handlers.push(notice.connect_clicked(move |_| {
                    emit(&dispatch, slot, key, "onActivate", vec![], None)
                }));
                notice.upcast()
            }
            Element::Calendar(_) => {
                let calendar = Calendar::new();
                let dispatch = self.dispatch.clone();
                let slot = self.slot;
                handlers.push(calendar.connect_month_shown(move |_, year, month| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onMonth",
                        vec![Value::from(year), Value::from(month)],
                        None,
                    )
                }));
                let dispatch = self.dispatch.clone();
                handlers.push(calendar.connect_day_selected(move |_, day| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onSelect",
                        vec![Value::String(format!(
                            "{:04}-{:02}-{:02}",
                            day.year, day.month, day.day
                        ))],
                        None,
                    )
                }));
                calendar.upcast()
            }
            Element::DropDown(_) => {
                let drop_down =
                    gtk4::DropDown::new(Some(gtk4::StringList::new(&[])), gtk4::Expression::NONE);
                let factory = option_factory();
                drop_down.set_factory(Some(&factory));
                drop_down.set_list_factory(Some(&factory));
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(drop_down.connect_selected_notify(move |drop_down| {
                    if drop_down.selected() != gtk4::INVALID_LIST_POSITION {
                        emit(
                            &dispatch,
                            slot,
                            key,
                            "onSelect",
                            vec![Value::from(drop_down.selected())],
                            Some(bump(&seq, key)),
                        );
                    }
                }));
                drop_down.upcast()
            }
            Element::ToggleButton(_) => {
                let button = gtk4::ToggleButton::new();
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(button.connect_toggled(move |button| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onToggle",
                        vec![Value::Bool(button.is_active())],
                        Some(bump(&seq, key)),
                    )
                }));
                button.upcast()
            }
            Element::CheckButton(_) => {
                let check = gtk4::CheckButton::new();
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(check.connect_toggled(move |check| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onToggle",
                        vec![Value::Bool(check.is_active())],
                        Some(bump(&seq, key)),
                    )
                }));
                check.upcast()
            }
            Element::Revealer(_) => {
                let revealer = gtk4::Revealer::new();
                let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                revealer.set_child(Some(&content));
                inner = Some(content);
                revealer.upcast()
            }
            Element::Stack(_) => {
                let stack = gtk4::Stack::new();
                stack.set_hhomogeneous(false);
                stack.set_vhomogeneous(false);
                stack.upcast()
            }
            Element::RangeBar(_) => RangeBar::new().upcast(),
            Element::Bars(_) => Bars::new().upcast(),
            Element::Sparkline(_) => Sparkline::new().upcast(),
            Element::Dots(_) => Dots::new().upcast(),
            Element::Placeholder(_) => Placeholder::new().upcast(),
            Element::Label(_) => gtk4::Label::new(None).upcast(),
            Element::Image(_) => gtk4::Image::new().upcast(),
            Element::Swatch(_) => {
                let swatch = Swatch::default();
                swatch.set_valign(gtk4::Align::Center);
                swatch.upcast()
            }
            Element::Button(_) => {
                let button = gtk4::Button::new();
                let dispatch = self.dispatch.clone();
                let slot = self.slot;
                handlers.push(
                    button.connect_clicked(move |_| {
                        emit(&dispatch, slot, key, "onClick", vec![], None)
                    }),
                );
                button.upcast()
            }
            Element::Switch(_) => {
                let switch = gtk4::Switch::new();
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(switch.connect_active_notify(move |switch| {
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onToggle",
                        vec![Value::Bool(switch.is_active())],
                        Some(bump(&seq, key)),
                    )
                }));
                switch.upcast()
            }
            Element::Scale(p) => {
                let scale = gtk4::Scale::with_range(
                    gtk4::Orientation::Horizontal,
                    p.min,
                    p.max,
                    render::scale_step(p.min, p.max, p.step),
                );
                scale.set_draw_value(false);
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(scale.connect_value_changed(move |scale| {
                    if let Some(value) = Number::from_f64(scale.value()) {
                        emit(
                            &dispatch,
                            slot,
                            key,
                            "onChange",
                            vec![Value::Number(value)],
                            Some(bump(&seq, key)),
                        );
                    }
                }));
                scale.upcast()
            }
            Element::Spinner(_) => adw::Spinner::new().upcast(),
            Element::Separator(_) => gtk4::Separator::new(gtk4::Orientation::Horizontal).upcast(),
            Element::Container(_) => {
                let container = Container::new();
                let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                container.set_child(Some(&content));
                inner = Some(content);
                container.upcast()
            }
            Element::Unsupported(_) => {
                let row = UiRow::new();
                row.set_inactive(true);
                row.upcast()
            }
            Element::Root | Element::Popover(_) | Element::Indicator(_) | Element::Addon(_) => {
                gtk4::Box::new(gtk4::Orientation::Vertical, 0).upcast()
            }
        };
        (widget, inner, handlers)
    }

    fn dress(&mut self, key: Key, node: &Arc<Node>, initial: bool) {
        let Some(mounted) = self.nodes.get_mut(&key) else {
            return;
        };
        if !initial
            && Arc::ptr_eq(&mounted.node, node)
            && !(self.tones_dirty.get()
                && matches!(node.element, Element::Dots(_) | Element::Calendar(_)))
        {
            return;
        }
        let widget = &mounted.widget.clone();
        let old_class = element_class(&mounted.node.element).map(render::class_name);
        let new_class = element_class(&node.element).map(render::class_name);
        if old_class != new_class {
            if let Some(class) = old_class {
                widget.remove_css_class(class);
            }
            if let Some(class) = new_class {
                widget.add_css_class(class);
            }
        } else if initial && let Some(class) = new_class {
            widget.add_css_class(class);
        }
        for handler in &mounted.handlers {
            widget.block_signal(handler);
        }
        match &node.element {
            Element::Section(p) => {
                widget.set_property("title", p.title.as_deref());
                widget.set_property("count", p.count.as_deref());
            }
            Element::Row(p) => {
                dress_static_row!(widget, p);
            }
            Element::ActionRow(p) => {
                dress_row_base!(widget, p);
            }
            Element::ToggleRow(p) => {
                dress_row_base!(widget, p);
                widget.set_property("control", control(p.control));
                if !render::keeps_newer_local_value(self.seq.borrow().get(&key).copied(), node.seq)
                {
                    widget.set_property("active", p.active);
                }
            }
            Element::MeterRow(p) => {
                dress_row_base!(widget, p);
                widget.set_property("level", p.level.unwrap_or(-1.0));
            }
            Element::ExpanderRow(p) => {
                dress_row_base!(widget, p);
                widget.set_property("tint", p.tint);
                widget.set_property("title-lines", p.title_lines);
                if !render::keeps_newer_local_value(self.seq.borrow().get(&key).copied(), node.seq)
                {
                    widget.set_property("open", p.open);
                }
            }
            Element::End(_) | Element::Start(_) => {}
            Element::SliderRow(p) => {
                widget.set_property("title", p.title.as_deref());
                widget.set_property("subtitle", p.subtitle.as_deref());
                widget.set_property("min", p.min);
                widget.set_property("max", p.max);
                widget.set_property("step", p.step);
                widget.set_property("tone", row_tone(p.tone));
                widget.set_property("inactive", p.inactive);
                widget.set_property("tooltip", p.tooltip.as_deref());
                if !render::keeps_newer_local_value(self.seq.borrow().get(&key).copied(), node.seq)
                {
                    widget.set_property("value", p.value.unwrap_or(p.min));
                }
            }
            Element::Entry(p) => {
                if let Some(entry) = widget.downcast_ref::<gtk4::Entry>() {
                    entry.set_placeholder_text(p.placeholder.as_deref());
                    if !render::keeps_newer_local_value(
                        self.seq.borrow().get(&key).copied(),
                        node.seq,
                    ) && entry.text() != p.value
                    {
                        let position = entry.position();
                        entry.set_text(&p.value);
                        entry.set_position(position.min(p.value.chars().count() as i32));
                    }
                }
            }
            Element::EntryRow(p) => {
                widget.set_property("title", p.title.as_deref());
                widget.set_property("placeholder", p.placeholder.as_deref());
                widget.set_property("tone", row_tone(p.tone));
                widget.set_property("inactive", p.inactive);
                widget.set_property("tooltip", p.tooltip.as_deref());
                if !render::keeps_newer_local_value(self.seq.borrow().get(&key).copied(), node.seq)
                {
                    widget.set_property("text", &p.value);
                }
            }
            Element::FactList(p) => {
                if let Some(list) = widget.downcast_ref::<gtk4::Box>() {
                    let facts: Vec<_> = p
                        .facts
                        .iter()
                        .map(|fact| {
                            if fact.warning {
                                glimpse_widgets::Fact::warning(&fact.label, &fact.value)
                            } else {
                                glimpse_widgets::Fact::new(&fact.label, &fact.value)
                            }
                        })
                        .collect();
                    glimpse_widgets::set_facts(list, &facts);
                }
            }
            Element::ChoiceList(p) => {
                if let Some(list) = widget.downcast_ref::<ChoiceList>() {
                    let choices: Vec<_> = p
                        .choices
                        .iter()
                        .map(|choice| glimpse_widgets::Choice {
                            label: choice.label.clone(),
                            detail: choice.detail.clone().unwrap_or_default(),
                            icon_name: choice.icon.clone().unwrap_or_default(),
                            warning: choice.warning,
                        })
                        .collect();
                    let local = list.selected();
                    list.set_choices(&choices);
                    if render::keeps_newer_local_value(
                        self.seq.borrow().get(&key).copied(),
                        node.seq,
                    ) {
                        list.set_selected(local);
                    } else {
                        list.set_selected(p.selected);
                    }
                }
            }
            Element::Readout(p) => {
                widget.set_property("value", p.value.as_deref());
                widget.set_property("unit", p.unit.as_deref());
            }
            Element::Notice(p) => {
                widget.set_property("icon-name", p.icon.as_deref());
                widget.set_property("title", p.title.as_deref());
                widget.set_property("subtitle", p.subtitle.as_deref());
                widget.set_property(
                    "severity",
                    match p.severity {
                        Some(glimpse_services::Severity::Warning) => {
                            glimpse_widgets::Severity::Warning
                        }
                        Some(glimpse_services::Severity::Error) => glimpse_widgets::Severity::Error,
                        _ => glimpse_widgets::Severity::Info,
                    },
                );
                widget.set_property("activatable", p.on_activate);
            }
            Element::Calendar(p) => {
                if let Some(calendar) = widget.downcast_ref::<Calendar>() {
                    dress_calendar(calendar, p);
                }
            }
            Element::DropDown(p) => {
                if let Some(drop_down) = widget.downcast_ref::<gtk4::DropDown>()
                    && let Some(model) = drop_down.model().and_downcast::<gtk4::StringList>()
                {
                    let current = drop_down.selected();
                    let same = model.n_items() as usize == p.options.len()
                        && p.options.iter().enumerate().all(|(index, option)| {
                            model.string(index as u32).as_deref() == Some(option.as_str())
                        });
                    if !same {
                        let options: Vec<&str> = p.options.iter().map(String::as_str).collect();
                        model.splice(0, model.n_items(), &options);
                    }
                    let count = p.options.len() as u32;
                    let wanted = if render::keeps_newer_local_value(
                        self.seq.borrow().get(&key).copied(),
                        node.seq,
                    ) {
                        (current < count).then_some(current)
                    } else {
                        p.selected
                            .filter(|index| *index < count)
                            .or((current < count).then_some(current))
                    };
                    let wanted = wanted.unwrap_or(gtk4::INVALID_LIST_POSITION);
                    if drop_down.selected() != wanted {
                        drop_down.set_selected(wanted);
                    }
                }
            }
            Element::ToggleButton(p) => {
                if let Some(toggle) = widget.downcast_ref::<gtk4::ToggleButton>() {
                    let content_changed = initial
                        || !matches!(&mounted.node.element, Element::ToggleButton(old) if old.label == p.label && old.icon == p.icon);
                    if content_changed {
                        set_button_content(
                            toggle.upcast_ref(),
                            p.label.as_deref(),
                            p.icon.as_deref(),
                        );
                    }
                    if let Some(label) = p.label.as_deref().or(p.icon.as_deref()) {
                        toggle.update_property(&[gtk4::accessible::Property::Label(label)]);
                    } else {
                        toggle.reset_property(gtk4::AccessibleProperty::Label);
                    }
                    if !render::keeps_newer_local_value(
                        self.seq.borrow().get(&key).copied(),
                        node.seq,
                    ) && toggle.is_active() != p.active
                    {
                        toggle.set_active(p.active);
                    }
                }
            }
            Element::CheckButton(p) => {
                if let Some(check) = widget.downcast_ref::<gtk4::CheckButton>() {
                    let label = check
                        .child()
                        .and_downcast::<gtk4::Label>()
                        .unwrap_or_else(|| {
                            let label = gtk4::Label::new(None);
                            label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                            label.set_max_width_chars(48);
                            label.set_halign(gtk4::Align::Start);
                            label.set_xalign(0.0);
                            label.set_accessible_role(gtk4::AccessibleRole::Presentation);
                            check.set_child(Some(&label));
                            label
                        });
                    let text = p.label.as_deref().unwrap_or("");
                    if label.text().as_str() != text {
                        label.set_text(text);
                    }
                    label.set_visible(p.label.is_some());
                    if let Some(name) = p.label.as_deref() {
                        check.update_property(&[gtk4::accessible::Property::Label(name)]);
                    } else {
                        check.reset_property(gtk4::AccessibleProperty::Label);
                    }
                    if !render::keeps_newer_local_value(
                        self.seq.borrow().get(&key).copied(),
                        node.seq,
                    ) && check.is_active() != p.active
                    {
                        check.set_active(p.active);
                    }
                }
            }
            Element::Revealer(p) => {
                if let Some(revealer) = widget.downcast_ref::<gtk4::Revealer>() {
                    revealer.set_transition_type(match p.transition {
                        Transition::SlideDown => gtk4::RevealerTransitionType::SlideDown,
                        Transition::Crossfade => gtk4::RevealerTransitionType::Crossfade,
                        Transition::None => gtk4::RevealerTransitionType::None,
                    });
                    if revealer.reveals_child() != p.revealed {
                        revealer.set_reveal_child(p.revealed);
                    }
                }
            }
            Element::Stack(_) => {}
            Element::Swatch(p) => {
                if let Some(swatch) = widget.downcast_ref::<Swatch>() {
                    let color = p
                        .color
                        .as_deref()
                        .and_then(|color| gdk::RGBA::parse(color).ok());
                    swatch.set_color(color.as_ref());
                    swatch.set_tooltip_text(p.tooltip.as_deref());
                    let large = p.size == SwatchSize::Large;
                    if swatch.has_css_class("swatch--hero") != large {
                        if large {
                            swatch.add_css_class("swatch--hero");
                        } else {
                            swatch.remove_css_class("swatch--hero");
                        }
                    }
                }
            }
            Element::Dots(p) => {
                if let Some(dots) = widget.downcast_ref::<Dots>() {
                    dots.set_colors(&paint_tones(&tone_palette(), &p.colors));
                    dots.set_uniform(p.uniform);
                    dots.set_size(p.size.map_or(DOTS_SIZE, |size| size as f32));
                    dots.set_max(p.max.map_or(usize::MAX, |max| max as usize));
                }
            }
            Element::Bars(p) => {
                if let Some(bars) = widget.downcast_ref::<Bars>() {
                    bars.set_values(&p.values);
                    bars.set_maximum(p.maximum);
                    bars.set_highlight_last(p.highlight_last);
                    bars.set_label(p.label.as_deref().unwrap_or_default());
                    bars.set_tooltips(&p.tooltips);
                }
            }
            Element::Sparkline(p) => {
                if let Some(line) = widget.downcast_ref::<Sparkline>() {
                    line.set_values(&p.values);
                    line.set_fill(p.fill);
                    line.set_show_last(p.show_last);
                    line.set_label(p.label.as_deref().unwrap_or_default());
                }
            }
            Element::RangeBar(p) => {
                if let Some(bar) = widget.downcast_ref::<RangeBar>() {
                    bar.set_scale(p.min, p.max);
                    bar.set_range(p.low, p.high);
                    bar.set_now(p.now);
                }
            }
            Element::Placeholder(p) => {
                widget.set_property("icon-name", p.icon.as_deref());
                widget.set_property("title", p.title.as_deref());
                widget.set_property("description", p.description.as_deref());
            }
            Element::Box(p) => {
                if let Some(box_) = widget.downcast_ref::<gtk4::Box>() {
                    box_.set_orientation(match p.orientation {
                        glimpse_services::Orientation::Horizontal => gtk4::Orientation::Horizontal,
                        glimpse_services::Orientation::Vertical => gtk4::Orientation::Vertical,
                    });
                    box_.set_spacing(p.spacing as i32);
                    box_.set_homogeneous(p.homogeneous);
                    box_.set_hexpand(p.hexpand);
                    box_.set_vexpand(p.vexpand);
                    box_.set_halign(p.halign.map(render::align).unwrap_or(gtk4::Align::Fill));
                    box_.set_valign(p.valign.map(render::align).unwrap_or(gtk4::Align::Fill));
                }
            }
            Element::Label(p) => {
                if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
                    label.set_text(p.text.as_deref().unwrap_or_default());
                    label.set_wrap(p.wrap);
                    label.set_max_width_chars(48);
                    label.set_lines(p.lines as i32);
                    label.set_ellipsize(match (p.wrap, p.lines) {
                        (true, 0) => gtk4::pango::EllipsizeMode::None,
                        (true, _) => gtk4::pango::EllipsizeMode::End,
                        (false, _) => render::ellipsize(p.ellipsize),
                    });
                    label.set_xalign(p.xalign.unwrap_or(0.5) as f32);
                }
            }
            Element::Image(p) => {
                if let Some(image) = widget.downcast_ref::<gtk4::Image>() {
                    image.set_icon_name(p.icon.as_deref());
                    image.set_pixel_size(p.pixel_size.map(|size| size as i32).unwrap_or(-1));
                    image.set_tooltip_text(p.tooltip.as_deref());
                    if let Some(tooltip) = p.tooltip.as_deref() {
                        image.update_property(&[gtk4::accessible::Property::Label(tooltip)]);
                    } else {
                        image.reset_property(gtk4::AccessibleProperty::Label);
                    }
                }
            }
            Element::Button(p) => {
                let content_changed = initial
                    || !matches!(&mounted.node.element, Element::Button(old) if old.text == p.text && old.icon == p.icon);
                if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
                    if content_changed {
                        set_button_content(button, p.text.as_deref(), p.icon.as_deref());
                    }
                    button.set_sensitive(p.sensitive);
                    let label = p.text.as_deref().or(p.tooltip.as_deref());
                    button.set_tooltip_text(p.tooltip.as_deref().or(p.text.as_deref()));
                    if let Some(label) = label {
                        button.update_property(&[gtk4::accessible::Property::Label(label)]);
                    } else {
                        button.reset_property(gtk4::AccessibleProperty::Label);
                    }
                }
            }
            Element::Switch(p) => {
                if let Some(switch) = widget.downcast_ref::<gtk4::Switch>() {
                    if !render::keeps_newer_local_value(
                        self.seq.borrow().get(&key).copied(),
                        node.seq,
                    ) && switch.is_active() != p.active
                    {
                        switch.set_active(p.active);
                    }
                    switch.set_sensitive(p.sensitive);
                }
            }
            Element::Scale(p) => {
                if let Some(scale) = widget.downcast_ref::<gtk4::Scale>() {
                    let adjustment = scale.adjustment();
                    if adjustment.lower() != p.min || adjustment.upper() != p.max {
                        scale.set_range(p.min, p.max);
                    }
                    let step = render::scale_step(p.min, p.max, p.step);
                    if adjustment.step_increment() != step {
                        scale.set_increments(step, step);
                    }
                    if !render::keeps_newer_local_value(
                        self.seq.borrow().get(&key).copied(),
                        node.seq,
                    ) && scale.value() != p.value
                    {
                        scale.set_value(p.value);
                    }
                    scale.set_sensitive(p.sensitive);
                }
            }
            Element::Separator(p) => {
                if let Some(separator) = widget.downcast_ref::<gtk4::Separator>() {
                    separator.set_orientation(match p.orientation {
                        glimpse_services::Orientation::Horizontal => gtk4::Orientation::Horizontal,
                        glimpse_services::Orientation::Vertical => gtk4::Orientation::Vertical,
                    });
                }
            }
            Element::Unsupported(name) => {
                widget.set_property(
                    "title",
                    Some(gettext("Unsupported: {name}").replace("{name}", name)),
                );
                widget.set_property(
                    "subtitle",
                    Some(gettext(
                        "This applet asked for an element glimpse does not render",
                    )),
                );
                widget.set_property("icon-name", Some("dialog-warning-symbolic"));
            }
            Element::Container(props) => {
                if let Some(container) = widget.downcast_ref::<Container>() {
                    container.set_width(props.width.unwrap_or(-1));
                    container.set_height(props.height.unwrap_or(-1));
                    container.set_background(paint(props.background));
                    container.set_color(paint(props.color));
                    container
                        .set_align(props.align.map(render::align).unwrap_or(gtk4::Align::Fill));
                }
            }
            Element::Root
            | Element::Popover(_)
            | Element::Indicator(_)
            | Element::Spinner(_)
            | Element::Addon(_) => {}
        }
        for handler in &mounted.handlers {
            widget.unblock_signal(handler);
        }
        mounted.node = node.clone();
    }
}

impl Drop for Dom {
    fn drop(&mut self) {
        for handler in self.style_handlers.drain(..) {
            self.style_manager.disconnect(handler);
        }
    }
}

fn row_tone(tone: ExecRowTone) -> RowTone {
    match tone {
        ExecRowTone::Normal => RowTone::Normal,
        ExecRowTone::Warning => RowTone::Warning,
        ExecRowTone::Error => RowTone::Error,
        ExecRowTone::Destructive => RowTone::Destructive,
    }
}

fn row_typography(typography: ExecTypography) -> RowTypography {
    match typography {
        ExecTypography::Default => RowTypography::Default,
        ExecTypography::Numeric => RowTypography::Numeric,
        ExecTypography::Monospace => RowTypography::Monospace,
    }
}

fn control(control: ExecControl) -> ToggleControl {
    match control {
        ExecControl::Check => ToggleControl::Check,
        ExecControl::Switch => ToggleControl::Switch,
    }
}

fn is_bare(kind: ElementKind) -> bool {
    !matches!(
        kind,
        ElementKind::Section
            | ElementKind::Row
            | ElementKind::ActionRow
            | ElementKind::ToggleRow
            | ElementKind::MeterRow
            | ElementKind::SliderRow
            | ElementKind::EntryRow
            | ElementKind::ExpanderRow
            | ElementKind::Separator
            | ElementKind::Revealer
            | ElementKind::FactList
            | ElementKind::Unsupported
    )
}

fn element_class(element: &Element) -> Option<ClassName> {
    match element {
        Element::Popover(p) => p.class_name,
        Element::Indicator(p) => p.class_name,
        Element::Section(p) => p.class_name,
        Element::Entry(p) => p.class_name,
        Element::Placeholder(p) => p.class_name,
        Element::Box(p) => p.class_name,
        Element::Label(p) => p.class_name,
        Element::Image(p) => p.class_name,
        Element::Button(p) => p.class_name,
        Element::Switch(p) => p.class_name,
        Element::Scale(p) => p.class_name,
        Element::Spinner(p) => p.class_name,
        Element::Separator(p) => p.class_name,
        Element::ChoiceList(p) => p.class_name,
        Element::Readout(p) => p.class_name,
        Element::Notice(p) => p.class_name,
        Element::Calendar(p) => p.class_name,
        Element::DropDown(p) => p.class_name,
        Element::ToggleButton(p) => p.class_name,
        Element::CheckButton(p) => p.class_name,
        Element::Revealer(p) => p.class_name,
        Element::Stack(p) => p.class_name,
        Element::RangeBar(p) => p.class_name,
        Element::Bars(p) => p.class_name,
        Element::Sparkline(p) => p.class_name,
        Element::Dots(p) => p.class_name,
        Element::Root
        | Element::FactList(_)
        | Element::Row(_)
        | Element::ActionRow(_)
        | Element::ToggleRow(_)
        | Element::MeterRow(_)
        | Element::ExpanderRow(_)
        | Element::End(_)
        | Element::Start(_)
        | Element::SliderRow(_)
        | Element::EntryRow(_)
        | Element::Unsupported(_)
        | Element::Addon(_)
        | Element::Swatch(_)
        | Element::Container(_) => None,
    }
}

fn paint(paint: Option<Paint>) -> ContainerPaint {
    match paint {
        Some(Paint::Red) => ContainerPaint::Red,
        Some(Paint::Orange) => ContainerPaint::Orange,
        Some(Paint::Yellow) => ContainerPaint::Yellow,
        Some(Paint::Green) => ContainerPaint::Green,
        Some(Paint::Blue) => ContainerPaint::Blue,
        Some(Paint::Purple) => ContainerPaint::Purple,
        Some(Paint::Light) => ContainerPaint::Light,
        Some(Paint::Dark) => ContainerPaint::Dark,
        None => ContainerPaint::None,
    }
}

fn set_button_content(button: &gtk4::Button, text: Option<&str>, icon: Option<&str>) {
    if let Some(text) = text {
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        if let Some(icon) = icon {
            content.append(&gtk4::Image::from_icon_name(icon));
        }
        let label = gtk4::Label::new(Some(text));
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_max_width_chars(48);
        content.append(&label);
        button.set_child(Some(&content));
    } else if let Some(icon) = icon {
        button.set_icon_name(icon);
    } else {
        button.set_child(None::<&gtk4::Widget>);
    }
}

fn ymd(date: chrono::NaiveDate) -> Ymd {
    use chrono::Datelike;
    Ymd::new(date.year(), date.month(), date.day())
}

fn tone_index(tone: Tone) -> usize {
    match tone {
        Tone::Accent => 0,
        Tone::Dim => 1,
        Tone::Success => 2,
        Tone::Warning => 3,
        Tone::Error => 4,
    }
}

fn tone_palette() -> [gdk::RGBA; 5] {
    let probe = |class_name: Option<&str>| {
        let label = gtk4::Label::new(None);
        if let Some(class_name) = class_name {
            label.add_css_class(class_name);
        }
        label.color()
    };
    let mut dim = probe(None);
    dim.set_alpha(DIM_ALPHA);
    [
        probe(Some("accent")),
        dim,
        probe(Some("success")),
        probe(Some("warning")),
        probe(Some("error")),
    ]
}

fn paint_tones(palette: &[gdk::RGBA; 5], colors: &[Tone]) -> Vec<gdk::RGBA> {
    colors
        .iter()
        .take(MARKER_LIMIT)
        .map(|tone| palette[tone_index(*tone)])
        .collect()
}

fn option_factory() -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::new(None);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_max_width_chars(OPTION_CHARS);
        label.set_xalign(0.0);
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let text = item
            .item()
            .and_then(|object| object.downcast::<gtk4::StringObject>().ok())
            .map(|object| object.string())
            .unwrap_or_default();
        label.set_text(&text);
        label.update_property(&[gtk4::accessible::Property::Label(text.as_str())]);
    });
    factory
}

fn dress_calendar(calendar: &Calendar, props: &CalendarProps) {
    if let (Some(year), Some(month)) = (props.year, props.month)
        && calendar.shown() != (year, month)
    {
        calendar.show_month(year, month);
    }
    if let Some(today) = props.today {
        calendar.set_today(ymd(today));
    }
    match props.selected {
        Some(date) if calendar.selected() != Some(ymd(date)) => calendar.select(ymd(date)),
        None => calendar.clear_selection(),
        _ => {}
    }
    let palette = tone_palette();
    let events: Vec<(Ymd, Vec<gdk::RGBA>)> = props
        .events
        .iter()
        .filter_map(|event| Some((ymd(event.date?), paint_tones(&palette, &event.colors))))
        .collect();
    calendar.set_events(&events);
}

fn swap_class(
    widget: &gtk4::Widget,
    current: &mut Option<&'static str>,
    next: Option<&'static str>,
) {
    if *current == next {
        return;
    }
    if let Some(class) = current.take() {
        widget.remove_css_class(class);
    }
    if let Some(class) = next {
        widget.add_css_class(class);
    }
    *current = next;
}

fn bump(seq: &Rc<RefCell<HashMap<Key, u64>>>, key: Key) -> u64 {
    let mut seq = seq.borrow_mut();
    let value = seq.entry(key).or_default();
    *value += 1;
    *value
}

fn emit(dispatch: &Dispatch, slot: u64, key: Key, name: &str, args: Vec<Value>, seq: Option<u64>) {
    dispatch(UserEvent {
        slot,
        generation: key.0,
        id: key.1,
        name: name.to_owned(),
        args,
        seq,
    });
}

#[cfg(test)]
mod tests;
