use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

use gettextrs::gettext;
use glimpse_services::{
    CalendarProps, ClassName, Element, ElementKind, ExecUserEvent as UserEvent, Node, ROOT, Tone,
    Transition, Tree,
};
use glimpse_widgets::{
    Bars, Calendar, ChoiceList, DOTS_SIZE, Dots, Expandable, FactList, Fader, Hero, Notice, Placeholder,
    PopoverShell, RangeBar, Readout, Row, Section, Sparkline, SwitchRow, Ymd, reconcile::by_key,
};
use gtk4::{gdk, glib, prelude::*};
use serde_json::{Number, Value};

use super::render;
use crate::applet::Opener;

pub(super) type Key = (u64, u32, ElementKind);
type Dispatch = Rc<dyn Fn(UserEvent)>;

const SLOT_SPACING: i32 = 6;
const PROGRESS_INSET: i32 = 13;
const PROGRESS_GAP: i32 = 6;

struct Mounted {
    widget: gtk4::Widget,
    inner: Option<gtk4::Box>,
    trail: Option<gtk4::Box>,
    node: Arc<Node>,
    handlers: Vec<glib::SignalHandlerId>,
}

pub struct Dom {
    shell: glib::WeakRef<PopoverShell>,
    closed: Rc<Cell<bool>>,
    popover_class: Option<&'static str>,
    footer_class: Option<&'static str>,
    body: gtk4::Box,
    footer_nodes: gtk4::Box,
    hero_box: gtk4::Box,
    hero: Option<Key>,
    nodes: HashMap<Key, Mounted>,
    held: HashMap<Key, Vec<(Key, gtk4::Widget)>>,
    seq: Rc<RefCell<HashMap<Key, u64>>>,
    dispatch: Dispatch,
    slot: u64,
    opener: Opener,
    generation: Option<u64>,
    footer_key: Option<Key>,
    applying: Rc<Cell<bool>>,
}

impl Dom {
    pub fn new(
        shell: &PopoverShell,
        slot: u64,
        dispatch: Dispatch,
        opener: Opener,
        seq: Rc<RefCell<HashMap<Key, u64>>>,
    ) -> Self {
        let hero_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        hero_box.set_visible(false);
        shell.set_hero(&hero_box);

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        body.set_visible(false);
        shell.set_content(&body);
        let footer_nodes = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        footer_nodes.set_visible(false);
        shell.append_to_footer(&footer_nodes);
        let closed = Rc::new(Cell::new(false));
        shell.connect_unmap({
            let closed = closed.clone();
            move |_| closed.set(true)
        });
        Self {
            shell: shell.downgrade(),
            closed,
            popover_class: None,
            footer_class: None,
            body,
            footer_nodes,
            hero_box,
            hero: None,
            nodes: HashMap::new(),
            held: HashMap::new(),
            seq,
            dispatch,
            slot,
            opener,
            generation: None,
            footer_key: None,
            applying: Rc::new(Cell::new(false)),
        }
    }

    pub fn is_alive(&self) -> bool {
        !self.closed.get() && self.shell.upgrade().is_some()
    }

    pub fn reconcile(&mut self, tree: &Tree, generation: u64) -> bool {
        self.applying.set(true);
        let alive = self.reconcile_tree(tree, generation);
        self.applying.set(false);
        alive
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
        let hero = render::first_child_of_kind(tree, popover, |element| {
            matches!(element, Element::Hero(_))
        });
        if let Some(id) = hero {
            if let Some((key, widget)) = self.mount(tree, generation, id, &mut live)
                && self.hero != Some(key)
            {
                if let Some(old) = self.hero.take().and_then(|key| self.nodes.get(&key)) {
                    self.hero_box.remove(&old.widget);
                }
                self.hero_box.append(&widget);
                self.hero = Some(key);
            }
        } else if let Some(key) = self.hero.take()
            && let Some(old) = self.nodes.get(&key)
        {
            self.hero_box.remove(&old.widget);
        }
        self.hero_box
            .set_visible(self.hero_box.first_child().is_some());
        let popover_class = tree
            .node(popover)
            .and_then(|node| element_class(&node.element))
            .map(render::class_name);
        swap_class(shell.upcast_ref(), &mut self.popover_class, popover_class);
        let root_key = (generation, ROOT, ElementKind::Popover);
        let body_ids: Vec<_> = children.iter().copied().filter(|id| matches!(tree.node(*id).map(|node| &node.element), Some(element) if !matches!(element, Element::Hero(_) | Element::Footer(_)))).collect();
        self.reconcile_children(
            tree,
            generation,
            root_key,
            &self.body.clone(),
            &body_ids,
            &mut live,
        );
        self.body.set_visible(self.body.first_child().is_some());
        if let Some(id) = render::first_child_of_kind(tree, popover, |element| {
            matches!(element, Element::Footer(_))
        }) {
            let key = (generation, id, ElementKind::Footer);
            let footer_class = tree
                .node(id)
                .and_then(|node| element_class(&node.element))
                .map(render::class_name);
            swap_class(
                self.footer_nodes.upcast_ref(),
                &mut self.footer_class,
                footer_class,
            );
            if self.footer_key != Some(key) {
                while let Some(child) = self.footer_nodes.first_child() {
                    self.footer_nodes.remove(&child);
                }
                self.footer_key = Some(key);
            }
            self.reconcile_children(
                tree,
                generation,
                key,
                &self.footer_nodes.clone(),
                tree.children(id),
                &mut live,
            );
        } else {
            while let Some(child) = self.footer_nodes.first_child() {
                self.footer_nodes.remove(&child);
            }
            self.footer_key = None;
            swap_class(self.footer_nodes.upcast_ref(), &mut self.footer_class, None);
        }
        self.footer_nodes
            .set_visible(self.footer_nodes.first_child().is_some());
        self.opener.typing(
            live.iter()
                .any(|key| matches!(key.2, ElementKind::Entry | ElementKind::Scale)),
        );
        self.nodes.retain(|key, _| live.contains(key));
        self.held.retain(|key, _| {
            key.0 == generation
                && (live.contains(key) || *key == root_key || Some(*key) == self.footer_key)
        });
        true
    }

    fn clear(&mut self) {
        while let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }
        while let Some(child) = self.footer_nodes.first_child() {
            self.footer_nodes.remove(&child);
        }
        if let Some(key) = self.hero.take()
            && let Some(old) = self.nodes.get(&key)
        {
            self.hero_box.remove(&old.widget);
        }
        self.body.set_visible(false);
        self.hero_box.set_visible(false);
        self.footer_nodes.set_visible(false);
        self.nodes.clear();
        self.held.clear();
        self.footer_key = None;
        swap_class(self.footer_nodes.upcast_ref(), &mut self.footer_class, None);
        if let Some(shell) = self.shell.upgrade() {
            swap_class(shell.upcast_ref(), &mut self.popover_class, None);
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
            let (widget, inner, trail, handlers) = self.build(&node, key);
            self.nodes.insert(
                key,
                Mounted {
                    widget,
                    inner,
                    trail,
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
            self.reconcile_children(tree, generation, key, &inner, &node.children, live);
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
    ) -> (
        gtk4::Widget,
        Option<gtk4::Box>,
        Option<gtk4::Box>,
        Vec<glib::SignalHandlerId>,
    ) {
        let mut handlers = Vec::new();
        let mut inner = None;
        let mut trail_box = None;
        let widget: gtk4::Widget = match &node.element {
            Element::Hero(_) => {
                let slot = gtk4::Box::new(gtk4::Orientation::Horizontal, SLOT_SPACING);
                inner = Some(slot);
                Hero::new().upcast()
            }
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
            Element::Row(_) => {
                let row = Row::new();
                let trail = gtk4::Box::new(gtk4::Orientation::Horizontal, SLOT_SPACING);
                let slot = gtk4::Box::new(gtk4::Orientation::Horizontal, SLOT_SPACING);
                let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
                chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
                chevron.add_css_class("drawer-chevron");
                trail.append(&slot);
                trail.append(&chevron);
                inner = Some(slot);
                trail_box = Some(trail);
                let dispatch = self.dispatch.clone();
                let slot = self.slot;
                handlers.push(row.connect_clicked(move |_| {
                    emit(&dispatch, slot, key, "onActivate", vec![], None, true)
                }));
                row.upcast()
            }
            Element::SwitchRow(_) => {
                let row = SwitchRow::new();
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
                        false,
                    )
                }));
                row.upcast()
            }
            Element::Fader(_) => {
                let fader = Fader::new();
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let slot = self.slot;
                handlers.push(fader.connect_changed(move |_, value| {
                    if let Some(value) = Number::from_f64(value) {
                        emit(
                            &dispatch,
                            slot,
                            key,
                            "onChange",
                            vec![Value::Number(value)],
                            Some(bump(&seq, key)),
                            false,
                        );
                    }
                }));
                let dispatch = self.dispatch.clone();
                handlers.push(fader.connect_toggled(move |_, _| {
                    emit(&dispatch, slot, key, "onMute", vec![], None, false)
                }));
                fader.upcast()
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
                        false,
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
                        true,
                    )
                }));
                entry.upcast()
            }
            Element::FactList(_) => FactList::new().upcast(),
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
                        true,
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
                    emit(&dispatch, slot, key, "onActivate", vec![], None, true)
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
                        true,
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
                        true,
                    )
                }));
                calendar.upcast()
            }
            Element::DropDown(_) => {
                let drop_down =
                    gtk4::DropDown::new(Some(gtk4::StringList::new(&[])), gtk4::Expression::NONE);
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
                            false,
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
                        false,
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
                        false,
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
            Element::Expander(_) => {
                let expandable = Expandable::new(&Row::new());
                let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                expandable.set_details(Some(&content));
                inner = Some(content);
                let dispatch = self.dispatch.clone();
                let seq = self.seq.clone();
                let applying = self.applying.clone();
                let slot = self.slot;
                handlers.push(expandable.connect_expanded_notify(move |expandable| {
                    if applying.get() {
                        return;
                    }
                    emit(
                        &dispatch,
                        slot,
                        key,
                        "onToggle",
                        vec![Value::Bool(expandable.expanded())],
                        Some(bump(&seq, key)),
                        false,
                    )
                }));
                expandable.upcast()
            }
            Element::RangeBar(_) => RangeBar::new().upcast(),
            Element::Bars(_) => Bars::new().upcast(),
            Element::Sparkline(_) => Sparkline::new().upcast(),
            Element::Dots(_) => Dots::new().upcast(),
            Element::Placeholder(_) => Placeholder::new().upcast(),
            Element::Label(_) => gtk4::Label::new(None).upcast(),
            Element::Image(_) => gtk4::Image::new().upcast(),
            Element::Button(_) => {
                let button = gtk4::Button::new();
                let dispatch = self.dispatch.clone();
                let slot = self.slot;
                handlers.push(button.connect_clicked(move |_| {
                    emit(&dispatch, slot, key, "onClick", vec![], None, true)
                }));
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
                        false,
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
                            false,
                        );
                    }
                }));
                scale.upcast()
            }
            Element::Spinner(_) => adw::Spinner::new().upcast(),
            Element::Progress(_) => {
                let progress = gtk4::ProgressBar::new();
                progress.set_margin_start(PROGRESS_INSET);
                progress.set_margin_end(PROGRESS_INSET);
                progress.set_margin_bottom(PROGRESS_GAP);
                progress.upcast()
            }
            Element::Separator(_) => gtk4::Separator::new(gtk4::Orientation::Horizontal).upcast(),
            Element::Unsupported(_) => {
                let row = Row::new();
                row.set_property("activatable", false);
                row.set_sensitive(false);
                row.upcast()
            }
            Element::Root | Element::Popover(_) | Element::Footer(_) | Element::Indicator(_) => {
                gtk4::Box::new(gtk4::Orientation::Vertical, 0).upcast()
            }
        };
        (widget, inner, trail_box, handlers)
    }

    fn dress(&mut self, key: Key, node: &Arc<Node>, initial: bool) {
        let Some(mounted) = self.nodes.get_mut(&key) else {
            return;
        };
        if !initial && Arc::ptr_eq(&mounted.node, node) {
            return;
        }
        let widget = &mounted.widget;
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
            Element::Hero(p) => {
                widget.set_property("title", p.title.as_deref());
                widget.set_property("subtitle", p.subtitle.as_deref());
                widget.set_property("icon-name", p.icon.as_deref());
                if let (Some(hero), Some(slot)) = (widget.downcast_ref::<Hero>(), &mounted.inner) {
                    if node.children.is_empty() {
                        hero.clear_slot();
                    } else if slot.parent().is_none() {
                        hero.set_slot(slot);
                    }
                }
            }
            Element::Section(p) => {
                widget.set_property("title", p.title.as_deref());
                widget.set_property("count", p.count.as_deref());
            }
            Element::Row(p) => {
                widget.set_property("title", p.title.as_deref());
                widget.set_property("subtitle", p.subtitle.as_deref());
                widget.set_property("lead-icon", p.icon.as_deref());
                widget.set_property("value", p.value.as_deref());
                widget.set_property("selectable", p.selected.is_some());
                widget.set_property("selected", p.selected.unwrap_or(false));
                widget.set_property("activatable", p.on_activate);
                widget.set_property("busy", p.busy);
                if let (Some(row), Some(trail)) = (widget.downcast_ref::<Row>(), &mounted.trail) {
                    if let Some(chevron) = trail.last_child() {
                        chevron.set_visible(p.chevron);
                    }
                    if node.children.is_empty() && !p.chevron {
                        row.clear_trail();
                    } else {
                        row.set_trail(trail);
                    }
                }
            }
            Element::SwitchRow(p) => {
                widget.set_property("title", p.title.as_deref());
                widget.set_property("subtitle", p.subtitle.as_deref());
                widget.set_property("lead-icon", p.icon.as_deref());
                if !render::keeps_newer_local_value(self.seq.borrow().get(&key).copied(), node.seq)
                {
                    widget.set_property("active", p.active);
                }
                widget.set_property("busy", p.busy);
            }
            Element::Fader(p) => {
                widget.set_property("icon-name", p.icon.as_deref());
                widget.set_property("maximum", p.maximum);
                widget.set_property("floor", p.floor);
                widget.set_property("toggleable", p.on_mute);
                widget.set_property("muted", p.muted);
                if !render::keeps_newer_local_value(self.seq.borrow().get(&key).copied(), node.seq)
                    && widget.property::<f64>("value") != p.value
                {
                    widget.set_property("value", p.value);
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
            Element::FactList(p) => {
                if let Some(list) = widget.downcast_ref::<FactList>() {
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
                    list.set_facts(&facts);
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
                    if let Some(label) = p.label.as_deref() {
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
                    check.set_use_underline(false);
                    if check.label().as_deref() != p.label.as_deref() {
                        check.set_label(p.label.as_deref());
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
            Element::Expander(p) => {
                if let Some(expandable) = widget.downcast_ref::<Expandable>() {
                    if let Some(head) = expandable.head::<Row>() {
                        head.set_title(p.title.as_deref());
                        head.set_subtitle(p.subtitle.as_deref());
                    }
                    if !render::keeps_newer_local_value(
                        self.seq.borrow().get(&key).copied(),
                        node.seq,
                    ) && expandable.expanded() != p.expanded
                    {
                        expandable.set_expanded(p.expanded);
                    }
                }
            }
            Element::Dots(p) => {
                if let Some(dots) = widget.downcast_ref::<Dots>() {
                    let colors: Vec<gdk::RGBA> =
                        p.colors.iter().map(|tone| tone_color(*tone)).collect();
                    dots.set_colors(&colors);
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
            Element::Progress(p) => {
                if let Some(progress) = widget.downcast_ref::<gtk4::ProgressBar>() {
                    progress.set_fraction(p.fraction);
                    progress.set_text(p.text.as_deref());
                    progress.set_show_text(p.text.is_some());
                    progress.set_ellipsize(gtk4::pango::EllipsizeMode::End);
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
                widget.set_property("lead-icon", Some("dialog-warning-symbolic"));
            }
            Element::Root
            | Element::Popover(_)
            | Element::Footer(_)
            | Element::Indicator(_)
            | Element::Spinner(_) => {}
        }
        for handler in &mounted.handlers {
            widget.unblock_signal(handler);
        }
        mounted.node = node.clone();
    }
}

fn element_class(element: &Element) -> Option<ClassName> {
    match element {
        Element::Popover(p) => p.class_name,
        Element::Footer(p) => p.class_name,
        Element::Indicator(p) => p.class_name,
        Element::Hero(p) => p.class_name,
        Element::Section(p) => p.class_name,
        Element::Row(p) => p.class_name,
        Element::SwitchRow(p) => p.class_name,
        Element::Fader(p) => p.class_name,
        Element::Entry(p) => p.class_name,
        Element::Placeholder(p) => p.class_name,
        Element::Box(p) => p.class_name,
        Element::Label(p) => p.class_name,
        Element::Image(p) => p.class_name,
        Element::Button(p) => p.class_name,
        Element::Switch(p) => p.class_name,
        Element::Scale(p) => p.class_name,
        Element::Spinner(p) => p.class_name,
        Element::Progress(p) => p.class_name,
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
        Element::Expander(p) => p.class_name,
        Element::RangeBar(p) => p.class_name,
        Element::Bars(p) => p.class_name,
        Element::Sparkline(p) => p.class_name,
        Element::Dots(p) => p.class_name,
        Element::Root | Element::FactList(_) | Element::Unsupported(_) => None,
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

fn tone_color(tone: Tone) -> gdk::RGBA {
    let probe = gtk4::Label::new(None);
    probe.add_css_class(match tone {
        Tone::Accent => "accent",
        Tone::Dim => "dim-label",
        Tone::Success => "success",
        Tone::Warning => "warning",
        Tone::Error => "error",
    });
    probe.color()
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
    let events: Vec<(Ymd, Vec<gdk::RGBA>)> = props
        .events
        .iter()
        .filter_map(|event| {
            Some((
                ymd(event.date?),
                event.colors.iter().map(|tone| tone_color(*tone)).collect(),
            ))
        })
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

fn emit(
    dispatch: &Dispatch,
    slot: u64,
    key: Key,
    name: &str,
    args: Vec<Value>,
    seq: Option<u64>,
    gesture: bool,
) {
    dispatch(UserEvent {
        slot,
        generation: key.0,
        id: key.1,
        name: name.to_owned(),
        args,
        seq,
        gesture,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_services::{Op, WireNode};
    use serde_json::json;

    fn init() {
        gtk4::init().expect("GTK display");
        glimpse_widgets::register_resources().expect("widget resources");
    }

    fn node(id: u32, kind: &str, props: Value, children: Vec<WireNode>) -> WireNode {
        WireNode {
            id,
            kind: kind.to_owned(),
            props: serde_json::from_value(props).expect("props"),
            children,
        }
    }

    fn tree(children: Vec<WireNode>) -> Tree {
        Tree::default()
            .apply(vec![Op::Insert {
                parent: 0,
                node: node(1, "popover", json!({}), children),
                before: None,
            }])
            .expect("tree")
    }

    fn setup(tree: &Tree) -> (PopoverShell, Dom, Rc<RefCell<Vec<UserEvent>>>) {
        let shell = PopoverShell::new();
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        let (sender, _receiver) = relm4::channel();
        let opener = crate::applet::Ctx::new("test".into(), None, sender).opener();
        let mut dom = Dom::new(
            &shell,
            7,
            Rc::new(move |event| seen.borrow_mut().push(event)),
            opener,
            Rc::new(RefCell::new(HashMap::new())),
        );
        assert!(dom.reconcile(tree, 1));
        (shell, dom, events)
    }

    fn widget(dom: &Dom, generation: u64, id: u32, kind: ElementKind) -> gtk4::Widget {
        dom.nodes
            .get(&(generation, id, kind))
            .expect("mounted widget")
            .widget
            .clone()
    }

    #[test]
    fn signal_dispatch_keeps_name_args_generation_and_gesture() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        let dispatch: Dispatch = Rc::new(move |event| seen.borrow_mut().push(event));
        emit(
            &dispatch,
            7,
            (3, 42, ElementKind::Button),
            "onClick",
            vec![json!("go")],
            Some(2),
            true,
        );
        let events = events.borrow();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].slot, 7);
        assert_eq!(events[0].generation, 3);
        assert_eq!(events[0].id, 42);
        assert_eq!(events[0].name, "onClick");
        assert_eq!(events[0].args, vec![json!("go")]);
        assert_eq!(events[0].seq, Some(2));
        assert!(events[0].gesture);
    }

    #[test]
    #[ignore = "needs a display"]
    fn empty_popover_later_shows_row() {
        init();
        let first = tree(vec![]);
        let (_shell, mut dom, _) = setup(&first);
        assert!(!dom.body.get_visible());
        assert!(dom.hero_box.first_child().is_none());
        assert!(!dom.hero_box.get_visible());
        assert!(dom.footer_nodes.first_child().is_none());
        assert!(!dom.footer_nodes.get_visible());
        let next = first
            .apply(vec![Op::Insert {
                parent: 1,
                node: node(2, "row", json!({"title":"Ready"}), vec![]),
                before: None,
            }])
            .expect("row");
        assert!(dom.reconcile(&next, 1));
        assert!(dom.body.get_visible());
        assert!(widget(&dom, 1, 2, ElementKind::Row).parent().is_some());
        assert!(dom.hero_box.first_child().is_none());
        assert!(!dom.hero_box.get_visible());
        let with_hero = next
            .apply(vec![Op::Insert {
                parent: 1,
                node: node(3, "hero", json!({"title":"Tasks"}), vec![]),
                before: Some(2),
            }])
            .expect("hero");
        assert!(dom.reconcile(&with_hero, 1));
        let hero = widget(&dom, 1, 3, ElementKind::Hero);
        assert_eq!(dom.hero_box.first_child(), Some(hero.clone()));
        assert!(hero.next_sibling().is_none());
        assert!(dom.hero_box.get_visible());
    }

    #[test]
    #[ignore = "needs a display"]
    fn reorder_keeps_widget_identity() {
        init();
        let first = tree(vec![
            node(2, "row", json!({"title":"A"}), vec![]),
            node(3, "row", json!({"title":"B"}), vec![]),
        ]);
        let (_shell, mut dom, _) = setup(&first);
        let a = widget(&dom, 1, 2, ElementKind::Row);
        let b = widget(&dom, 1, 3, ElementKind::Row);
        let next = first
            .apply(vec![Op::Move {
                parent: 1,
                id: 3,
                before: Some(2),
            }])
            .expect("move");
        assert!(dom.reconcile(&next, 1));
        assert_eq!(widget(&dom, 1, 2, ElementKind::Row), a);
        assert_eq!(widget(&dom, 1, 3, ElementKind::Row), b);
        assert_eq!(dom.body.first_child(), Some(b));
    }

    #[test]
    #[ignore = "needs a display"]
    fn set_dresses_only_touched_widget() {
        init();
        let first = tree(vec![
            node(2, "row", json!({"title":"A"}), vec![]),
            node(3, "row", json!({"title":"B"}), vec![]),
        ]);
        let (_shell, mut dom, _) = setup(&first);
        let a = widget(&dom, 1, 2, ElementKind::Row);
        let b = widget(&dom, 1, 3, ElementKind::Row);
        let notifications = Rc::new(RefCell::new(0));
        let count = notifications.clone();
        b.connect_notify_local(Some("title"), move |_, _| *count.borrow_mut() += 1);
        let next = first
            .apply(vec![Op::Set {
                id: 2,
                props: serde_json::from_value(json!({"title":"C"})).expect("props"),
                seq: None,
            }])
            .expect("set");
        assert!(dom.reconcile(&next, 1));
        assert_eq!(widget(&dom, 1, 2, ElementKind::Row), a);
        assert_eq!(widget(&dom, 1, 3, ElementKind::Row), b);
        assert_eq!(*notifications.borrow(), 0);
        assert_eq!(a.property::<Option<String>>("title").as_deref(), Some("C"));
    }

    #[test]
    #[ignore = "needs a display"]
    fn text_button_keeps_its_label_with_icon() {
        init();
        let first = tree(vec![node(
            2,
            "button",
            json!({"text":"Run","icon":"media-playback-start-symbolic"}),
            vec![],
        )]);
        let (_shell, dom, _) = setup(&first);
        let button = widget(&dom, 1, 2, ElementKind::Button)
            .downcast::<gtk4::Button>()
            .expect("button");
        let content = button.child().and_downcast::<gtk4::Box>().expect("content");
        let label = content
            .last_child()
            .and_downcast::<gtk4::Label>()
            .expect("label");
        assert_eq!(label.text(), "Run");
        assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::End);
        assert_eq!(label.max_width_chars(), 48);
        assert!(gtk4::test_accessible_has_property(
            &button,
            gtk4::AccessibleProperty::Label
        ));
    }

    #[test]
    #[ignore = "needs a display"]
    fn row_handler_is_connected_once_and_survives_a_set() {
        init();
        let first = tree(vec![node(2, "row", json!({"title":"Open"}), vec![])]);
        let (_shell, mut dom, events) = setup(&first);
        let row = widget(&dom, 1, 2, ElementKind::Row);
        row.emit_by_name::<()>("clicked", &[]);
        assert_eq!(events.borrow().len(), 1);
        let next = first
            .apply(vec![Op::Set {
                id: 2,
                props: serde_json::from_value(json!({"onActivate":true})).expect("props"),
                seq: None,
            }])
            .expect("set");
        assert!(dom.reconcile(&next, 1));
        assert_eq!(widget(&dom, 1, 2, ElementKind::Row), row);
        row.emit_by_name::<()>("clicked", &[]);
        assert_eq!(events.borrow().len(), 2);
        assert_eq!(events.borrow()[1].name, "onActivate");
    }

    #[test]
    #[ignore = "needs a display"]
    fn moving_row_to_earlier_section_keeps_it_visible() {
        init();
        let first = tree(vec![
            node(2, "section", json!({"title":"A"}), vec![]),
            node(
                3,
                "section",
                json!({"title":"B"}),
                vec![node(4, "row", json!({"title":"Moved"}), vec![])],
            ),
        ]);
        let (_shell, mut dom, _) = setup(&first);
        let row = widget(&dom, 1, 4, ElementKind::Row);
        let next = first
            .apply(vec![Op::Move {
                parent: 2,
                id: 4,
                before: None,
            }])
            .expect("move");
        assert!(dom.reconcile(&next, 1));
        assert_eq!(widget(&dom, 1, 4, ElementKind::Row), row);
        assert_eq!(
            row.parent(),
            dom.nodes
                .get(&(1, 2, ElementKind::Section))
                .and_then(|m| m.inner.as_ref())
                .map(|inner| inner.clone().upcast())
        );
    }

    #[test]
    #[ignore = "needs a display"]
    fn seq_survives_dom_reopen() {
        init();
        let first = tree(vec![node(2, "switch", json!({"onToggle":true}), vec![])]);
        let (shell, dom, events) = setup(&first);
        let seq = dom.seq.clone();
        widget(&dom, 1, 2, ElementKind::Switch)
            .downcast::<gtk4::Switch>()
            .expect("switch")
            .set_active(true);
        let prior = events.borrow()[0].seq.expect("seq");
        drop(dom);
        let seen = events.clone();
        let (sender, _receiver) = relm4::channel();
        let opener = crate::applet::Ctx::new("test".into(), None, sender).opener();
        let mut reopened = Dom::new(
            &shell,
            7,
            Rc::new(move |event| seen.borrow_mut().push(event)),
            opener,
            seq,
        );
        assert!(reopened.reconcile(&first, 1));
        widget(&reopened, 1, 2, ElementKind::Switch)
            .downcast::<gtk4::Switch>()
            .expect("switch")
            .set_active(true);
        assert!(
            events
                .borrow()
                .last()
                .and_then(|event| event.seq)
                .expect("seq")
                > prior
        );
    }

    #[test]
    #[ignore = "needs a display"]
    fn hostile_text_and_icon_only_button() {
        init();
        let first = tree(vec![
            node(
                2,
                "label",
                json!({"text":"Long","ellipsize":"none"}),
                vec![],
            ),
            node(
                3,
                "progress",
                json!({"text":"Loading","fraction":0.5}),
                vec![],
            ),
            node(
                4,
                "button",
                json!({"icon":"media-playback-start-symbolic","tooltip":"Play"}),
                vec![],
            ),
            node(
                5,
                "image",
                json!({"icon":"dialog-warning-symbolic","tooltip":"Warning"}),
                vec![],
            ),
        ]);
        let (_shell, dom, _) = setup(&first);
        let label = widget(&dom, 1, 2, ElementKind::Label)
            .downcast::<gtk4::Label>()
            .expect("label");
        assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::None);
        let progress = widget(&dom, 1, 3, ElementKind::Progress)
            .downcast::<gtk4::ProgressBar>()
            .expect("progress");
        assert_eq!(progress.ellipsize(), gtk4::pango::EllipsizeMode::End);
        assert_eq!(progress.margin_start(), PROGRESS_INSET);
        assert_eq!(progress.margin_end(), PROGRESS_INSET);
        assert_eq!(progress.margin_bottom(), PROGRESS_GAP);
        let button = widget(&dom, 1, 4, ElementKind::Button)
            .downcast::<gtk4::Button>()
            .expect("button");
        assert_eq!(button.tooltip_text().as_deref(), Some("Play"));
        assert!(gtk4::test_accessible_has_property(
            &button,
            gtk4::AccessibleProperty::Label
        ));
        let image = widget(&dom, 1, 5, ElementKind::Image)
            .downcast::<gtk4::Image>()
            .expect("image");
        assert_eq!(image.tooltip_text().as_deref(), Some("Warning"));
        assert!(gtk4::test_accessible_has_property(
            &image,
            gtk4::AccessibleProperty::Label
        ));
    }

    #[test]
    #[ignore = "needs a display"]
    fn unsupported_row_matches_warning_design() {
        init();
        let first = tree(vec![node(2, "canvas", json!({}), vec![])]);
        let (_shell, dom, _) = setup(&first);
        let row = widget(&dom, 1, 2, ElementKind::Unsupported)
            .downcast::<Row>()
            .expect("row");
        assert_eq!(
            row.property::<Option<String>>("title"),
            Some(gettext("Unsupported: {name}").replace("{name}", "canvas"))
        );
        assert_eq!(
            row.property::<Option<String>>("subtitle"),
            Some(gettext(
                "This applet asked for an element glimpse does not render"
            ))
        );
        assert_eq!(
            row.property::<Option<String>>("lead-icon").as_deref(),
            Some("dialog-warning-symbolic")
        );
    }

    #[test]
    #[ignore = "needs a display"]
    fn reload_replaces_widgets() {
        init();
        let first = tree(vec![node(2, "row", json!({"title":"A"}), vec![])]);
        let (_shell, mut dom, _) = setup(&first);
        let old = widget(&dom, 1, 2, ElementKind::Row);
        assert!(dom.reconcile(&first, 2));
        assert_ne!(widget(&dom, 2, 2, ElementKind::Row), old);
        assert!(old.parent().is_none());
    }

    #[test]
    #[ignore = "needs a display"]
    fn dressing_controls_emits_no_events() {
        init();
        let first = tree(vec![
            node(2, "switch", json!({"active":false,"onToggle":true}), vec![]),
            node(
                3,
                "scale",
                json!({"min":0,"max":100,"step":1,"value":4,"onChange":true}),
                vec![],
            ),
            node(
                4,
                "entry",
                json!({"value":"A","onChange":true,"onSubmit":true}),
                vec![],
            ),
            node(5, "row", json!({"title":"Open","onActivate":true}), vec![]),
            node(6, "button", json!({"text":"Run","onClick":true}), vec![]),
            node(
                7,
                "fader",
                json!({"value":1,"maximum":10,"onChange":true,"onMute":true}),
                vec![],
            ),
            node(
                8,
                "switchrow",
                json!({"title":"Mode","onToggle":true}),
                vec![],
            ),
            node(9, "scale", json!({"min":0,"max":1,"value":0.5}), vec![]),
        ]);
        let (_shell, mut dom, events) = setup(&first);
        let next = first
            .apply(vec![
                Op::Set {
                    id: 2,
                    props: serde_json::from_value(json!({"active":true})).expect("props"),
                    seq: None,
                },
                Op::Set {
                    id: 3,
                    props: serde_json::from_value(json!({"value":50})).expect("props"),
                    seq: None,
                },
                Op::Set {
                    id: 4,
                    props: serde_json::from_value(json!({"value":"B"})).expect("props"),
                    seq: None,
                },
            ])
            .expect("set");
        assert!(dom.reconcile(&next, 1));
        assert!(events.borrow().is_empty());
        widget(&dom, 1, 2, ElementKind::Switch)
            .downcast::<gtk4::Switch>()
            .expect("switch")
            .set_active(false);
        assert_eq!(events.borrow().len(), 1);
        assert_eq!(events.borrow()[0].name, "onToggle");
        assert!(!events.borrow()[0].gesture);
        widget(&dom, 1, 3, ElementKind::Scale)
            .downcast::<gtk4::Scale>()
            .expect("scale")
            .set_value(60.0);
        let entry = widget(&dom, 1, 4, ElementKind::Entry)
            .downcast::<gtk4::Entry>()
            .expect("entry");
        entry.emit_by_name::<()>("changed", &[]);
        entry.emit_by_name::<()>("activate", &[]);
        widget(&dom, 1, 5, ElementKind::Row).emit_by_name::<()>("clicked", &[]);
        widget(&dom, 1, 6, ElementKind::Button).emit_by_name::<()>("clicked", &[]);
        widget(&dom, 1, 7, ElementKind::Fader).emit_by_name::<()>("changed", &[&2.0f64]);
        widget(&dom, 1, 7, ElementKind::Fader).emit_by_name::<()>("toggled", &[&true]);
        widget(&dom, 1, 8, ElementKind::SwitchRow).emit_by_name::<()>("toggled", &[&true]);
        let events = events.borrow();
        assert_eq!(
            events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            [
                "onToggle",
                "onChange",
                "onChange",
                "onSubmit",
                "onActivate",
                "onClick",
                "onChange",
                "onMute",
                "onToggle"
            ]
        );
        assert_eq!(events[1].args, vec![json!(60.0)]);
        assert_eq!(events[2].args, vec![json!("B")]);
        assert_eq!(events[3].args, vec![json!("B")]);
        assert!(events[3].gesture && events[4].gesture && events[5].gesture);
        assert!(!events[6].gesture && !events[7].gesture && !events[8].gesture);
    }

    #[test]
    #[ignore = "needs a display"]
    fn older_seq_keeps_entry_text_and_caret() {
        init();
        let first = tree(vec![node(
            2,
            "entry",
            json!({"value":"A","onChange":true}),
            vec![],
        )]);
        let (_shell, mut dom, events) = setup(&first);
        let entry = widget(&dom, 1, 2, ElementKind::Entry)
            .downcast::<gtk4::Entry>()
            .expect("entry");
        entry.set_text("Ab");
        entry.set_position(1);
        assert_eq!(events.borrow()[0].seq, Some(1));
        let next = first
            .apply(vec![Op::Set {
                id: 2,
                props: serde_json::from_value(json!({"value":"A"})).expect("props"),
                seq: Some(0),
            }])
            .expect("set");
        assert!(dom.reconcile(&next, 1));
        assert_eq!(entry.text(), "Ab");
        assert_eq!(entry.position(), 1);
    }

    fn set(id: u32, props: Value) -> Op {
        Op::Set {
            id,
            props: serde_json::from_value(props).expect("props"),
            seq: None,
        }
    }

    #[test]
    #[ignore = "needs a display"]
    fn widget_elements_render_update_without_echo_and_emit_user_events() {
        init();
        let label = |id: u32| node(id, "label", json!({"text":"t"}), vec![]);
        let first = tree(vec![
            node(
                2,
                "factlist",
                json!({"facts":[{"label":"A","value":"1"}]}),
                vec![],
            ),
            node(
                3,
                "choicelist",
                json!({"choices":[{"label":"x"},{"label":"y"}],"selected":0,"onSelect":true}),
                vec![],
            ),
            node(4, "readout", json!({"value":"5","unit":"C"}), vec![]),
            node(
                5,
                "notice",
                json!({"title":"T","severity":"warning","onActivate":true}),
                vec![],
            ),
            node(
                6,
                "calendar",
                json!({"year":2026,"month":6,"selected":"2026-06-04","today":"2026-06-01",
                    "events":[{"date":"2026-06-05","colors":["error"]}],"onMonth":true,"onSelect":true}),
                vec![],
            ),
            node(
                7,
                "dots",
                json!({"colors":["accent","error"],"size":6}),
                vec![],
            ),
            node(
                8,
                "dropdown",
                json!({"options":["a","b","c"],"selected":1,"onSelect":true}),
                vec![],
            ),
            node(
                9,
                "togglebutton",
                json!({"label":"T","active":true,"onToggle":true}),
                vec![],
            ),
            node(
                10,
                "checkbutton",
                json!({"label":"C","onToggle":true}),
                vec![],
            ),
            node(
                11,
                "revealer",
                json!({"revealed":true,"transition":"none"}),
                vec![label(12)],
            ),
            node(
                13,
                "stack",
                json!({"visible":1}),
                vec![label(14), label(15)],
            ),
            node(
                16,
                "expander",
                json!({"title":"E","expanded":true,"onToggle":true}),
                vec![label(17)],
            ),
            node(
                18,
                "rangebar",
                json!({"min":0,"max":10,"low":2,"high":4,"now":3}),
                vec![],
            ),
            node(
                19,
                "progress",
                json!({"fraction":0.5,"className":"warning"}),
                vec![],
            ),
            node(
                20,
                "bars",
                json!({"values":[1,null,3],"maximum":5,"highlightLast":false,"label":"Cost","tooltips":["a",null,"c"]}),
                vec![],
            ),
            node(
                21,
                "sparkline",
                json!({"values":[1,2,null,4],"fill":true,"showLast":true,"label":"Trend"}),
                vec![],
            ),
        ]);
        let (_shell, mut dom, events) = setup(&first);
        assert!(events.borrow().is_empty());
        let get = |dom: &Dom, id: u32, kind: ElementKind| widget(dom, 1, id, kind);

        let list = get(&dom, 3, ElementKind::ChoiceList)
            .downcast::<ChoiceList>()
            .expect("list");
        assert_eq!(list.selected(), Some(0));
        let readout = get(&dom, 4, ElementKind::Readout);
        assert_eq!(
            readout.property::<Option<String>>("value").as_deref(),
            Some("5")
        );
        let notice = get(&dom, 5, ElementKind::Notice);
        assert_eq!(
            notice.property::<glimpse_widgets::Severity>("severity"),
            glimpse_widgets::Severity::Warning
        );
        assert!(notice.property::<bool>("activatable"));
        let calendar = get(&dom, 6, ElementKind::Calendar)
            .downcast::<Calendar>()
            .expect("calendar");
        assert_eq!(calendar.shown(), (2026, 6));
        assert_eq!(calendar.selected(), Some(Ymd::new(2026, 6, 4)));
        assert_eq!(calendar.events(Ymd::new(2026, 6, 5)).len(), 1);
        let drop_down = get(&dom, 8, ElementKind::DropDown)
            .downcast::<gtk4::DropDown>()
            .expect("dropdown");
        assert_eq!(drop_down.selected(), 1);
        let toggle = get(&dom, 9, ElementKind::ToggleButton)
            .downcast::<gtk4::ToggleButton>()
            .expect("toggle");
        assert!(toggle.is_active());
        let check = get(&dom, 10, ElementKind::CheckButton)
            .downcast::<gtk4::CheckButton>()
            .expect("check");
        assert!(!check.is_active());
        assert_eq!(check.label().as_deref(), Some("C"));
        let revealer = get(&dom, 11, ElementKind::Revealer)
            .downcast::<gtk4::Revealer>()
            .expect("revealer");
        assert!(revealer.reveals_child());
        assert!(get(&dom, 12, ElementKind::Label).parent().is_some());
        let stack = get(&dom, 13, ElementKind::Stack)
            .downcast::<gtk4::Stack>()
            .expect("stack");
        assert_eq!(stack.visible_child_name().as_deref(), Some("1"));
        let expander = get(&dom, 16, ElementKind::Expander)
            .downcast::<Expandable>()
            .expect("expander");
        assert!(expander.expanded());
        assert_eq!(
            expander
                .head::<Row>()
                .expect("head")
                .property::<Option<String>>("title")
                .as_deref(),
            Some("E")
        );
        let bars = get(&dom, 20, ElementKind::Bars)
            .downcast::<Bars>()
            .expect("bars");
        assert_eq!(bars.values(), vec![Some(1.0), None, Some(3.0)]);
        assert_eq!(bars.maximum(), Some(5.0));
        assert!(!bars.highlight_last());
        assert_eq!(bars.label(), "Cost");
        let line = get(&dom, 21, ElementKind::Sparkline)
            .downcast::<Sparkline>()
            .expect("sparkline");
        assert_eq!(line.values(), vec![Some(1.0), Some(2.0), None, Some(4.0)]);
        assert!(line.fill() && line.show_last());
        assert_eq!(line.label(), "Trend");
        let meter = get(&dom, 19, ElementKind::Progress)
            .downcast::<gtk4::ProgressBar>()
            .expect("meter");
        assert!(meter.has_css_class("warning"));

        let next = first
            .apply(vec![
                set(3, json!({"selected":1})),
                set(6, json!({"month":7,"selected":"2026-07-09"})),
                set(8, json!({"options":["x","y"],"selected":0})),
                set(9, json!({"active":false,"label":"U"})),
                set(10, json!({"active":true})),
                set(11, json!({"revealed":false})),
                set(13, json!({"visible":0})),
                set(16, json!({"expanded":false})),
                set(19, json!({"className":"error","fraction":0.25})),
            ])
            .expect("set");
        assert!(dom.reconcile(&next, 1));
        assert!(
            events.borrow().is_empty(),
            "{:?}",
            events.borrow().iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(list.selected(), Some(1));
        assert_eq!(calendar.shown(), (2026, 7));
        assert_eq!(drop_down.selected(), 0);
        assert!(!toggle.is_active() && check.is_active());
        assert!(!revealer.reveals_child());
        assert_eq!(stack.visible_child_name().as_deref(), Some("0"));
        assert!(!expander.expanded());
        assert!(meter.has_css_class("error") && !meter.has_css_class("warning"));
        assert_eq!(meter.fraction(), 0.25);

        list.emit_by_name::<()>("activated", &[&0u32]);
        notice.emit_by_name::<()>("clicked", &[]);
        calendar.emit_by_name::<()>("month-shown", &[&2026i32, &8u32]);
        calendar.emit_by_name::<()>("day-selected", &[&2026i32, &8u32, &9u32]);
        drop_down.set_selected(1);
        toggle.set_active(true);
        check.set_active(false);
        expander.set_expanded(true);
        let events = events.borrow();
        assert_eq!(
            events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            [
                "onSelect",
                "onActivate",
                "onMonth",
                "onSelect",
                "onSelect",
                "onToggle",
                "onToggle",
                "onToggle"
            ]
        );
        assert_eq!(events[0].args, vec![json!(0)]);
        assert_eq!(events[2].args, vec![json!(2026), json!(8)]);
        assert_eq!(events[3].args, vec![json!("2026-08-09")]);
        assert_eq!(events[4].args, vec![json!(1)]);
        assert_eq!(events[5].args, vec![json!(true)]);
        assert_eq!(events[7].args, vec![json!(true)]);
    }

    #[test]
    #[ignore = "needs a display"]
    fn row_and_hero_end_children_and_chevron() {
        init();
        let first = tree(vec![
            node(
                2,
                "row",
                json!({"title":"R","chevron":true}),
                vec![node(3, "label", json!({"text":"end"}), vec![])],
            ),
            node(
                4,
                "hero",
                json!({"title":"H"}),
                vec![node(5, "label", json!({"text":"end"}), vec![])],
            ),
        ]);
        let (_shell, mut dom, _) = setup(&first);
        let row = widget(&dom, 1, 2, ElementKind::Row)
            .downcast::<Row>()
            .expect("row");
        let end = widget(&dom, 1, 3, ElementKind::Label);
        assert!(row.trail().is_some());
        assert!(end.is_ancestor(&row));
        let hero = widget(&dom, 1, 4, ElementKind::Hero);
        assert!(widget(&dom, 1, 5, ElementKind::Label).is_ancestor(&hero));
        let next = first
            .apply(vec![
                set(2, json!({"chevron":false})),
                Op::Remove { parent: 2, id: 3 },
                Op::Remove { parent: 4, id: 5 },
            ])
            .expect("set");
        assert!(dom.reconcile(&next, 1));
        assert!(row.trail().is_none());
        assert!(!end.is_ancestor(&row));
    }

    #[test]
    #[ignore = "needs a display"]
    fn dropping_shell_drops_the_next_dom() {
        init();
        let first = tree(vec![node(2, "row", json!({"title":"A"}), vec![])]);
        let (shell, mut dom, _) = setup(&first);
        let child = widget(&dom, 1, 2, ElementKind::Row);
        let weak = child.downgrade();
        drop(child);
        drop(shell);
        assert!(!dom.reconcile(&first, 1));
        drop(dom);
        assert!(weak.upgrade().is_none());
    }
}
