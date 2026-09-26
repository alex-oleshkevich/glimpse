use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

use gettextrs::gettext;
use glimpse_services::{ClassName, Element, ElementKind, ExecUserEvent as UserEvent, Node, Tree};
use glimpse_widgets::{
    Fader, Hero, Placeholder, PopoverShell, Row, Section, SwitchRow, reconcile::by_key,
};
use gtk4::{glib, prelude::*};
use serde_json::Value;

use super::render;
use crate::applet::Opener;

pub(super) type Key = (u64, u32, ElementKind);
type Dispatch = Rc<dyn Fn(UserEvent)>;

struct Mounted {
    widget: gtk4::Widget,
    inner: Option<gtk4::Box>,
    node: Arc<Node>,
    handlers: Vec<glib::SignalHandlerId>,
}

pub struct Dom {
    shell: glib::WeakRef<PopoverShell>,
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
        Self {
            shell: shell.downgrade(),
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
        }
    }

    pub fn is_alive(&self) -> bool {
        self.shell.upgrade().is_some()
    }

    pub fn reconcile(&mut self, tree: &Tree, generation: u64) -> bool {
        if self.shell.upgrade().is_none() {
            return false;
        }
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
        let root_key = (generation, popover, ElementKind::Popover);
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
        let rebuild = self
            .nodes
            .get(&key)
            .is_some_and(|mounted| handlers_changed(&mounted.node.element, &node.element));
        if rebuild && let Some(old) = self.nodes.remove(&key) {
            old.widget.unparent();
        }
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
            self.reconcile_children(tree, generation, key, &inner, &node.children, live);
        }
        Some((key, widget))
    }

    fn build(
        &self,
        node: &Node,
        key: Key,
    ) -> (gtk4::Widget, Option<gtk4::Box>, Vec<glib::SignalHandlerId>) {
        let mut handlers = Vec::new();
        let mut inner = None;
        let widget: gtk4::Widget = match &node.element {
            Element::Hero(_) => Hero::new().upcast(),
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
            Element::Row(p) => {
                let row = Row::new();
                if p.on_activate {
                    let dispatch = self.dispatch.clone();
                    let slot = self.slot;
                    handlers.push(row.connect_clicked(move |_| {
                        emit(&dispatch, slot, key, "onActivate", vec![], None, true)
                    }));
                }
                row.upcast()
            }
            Element::SwitchRow(p) => {
                let row = SwitchRow::new();
                if p.on_toggle {
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
                }
                row.upcast()
            }
            Element::Fader(p) => {
                let fader = Fader::new();
                if p.on_change {
                    let dispatch = self.dispatch.clone();
                    let seq = self.seq.clone();
                    let slot = self.slot;
                    handlers.push(fader.connect_changed(move |_, value| {
                        if let Some(value) = render::number(value) {
                            emit(
                                &dispatch,
                                slot,
                                key,
                                "onChange",
                                vec![value],
                                Some(bump(&seq, key)),
                                false,
                            );
                        }
                    }));
                }
                if p.on_mute {
                    let dispatch = self.dispatch.clone();
                    let slot = self.slot;
                    handlers.push(fader.connect_toggled(move |_, _| {
                        emit(&dispatch, slot, key, "onMute", vec![], None, false)
                    }));
                }
                fader.upcast()
            }
            Element::Entry(p) => {
                let entry = gtk4::Entry::new();
                if p.on_change {
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
                }
                if p.on_submit {
                    let dispatch = self.dispatch.clone();
                    let slot = self.slot;
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
                }
                entry.upcast()
            }
            Element::Placeholder(_) => Placeholder::new().upcast(),
            Element::Label(_) => gtk4::Label::new(None).upcast(),
            Element::Image(_) => gtk4::Image::new().upcast(),
            Element::Button(p) => {
                let button = gtk4::Button::new();
                if p.on_click {
                    let dispatch = self.dispatch.clone();
                    let slot = self.slot;
                    handlers.push(button.connect_clicked(move |_| {
                        emit(&dispatch, slot, key, "onClick", vec![], None, true)
                    }));
                }
                button.upcast()
            }
            Element::Switch(p) => {
                let switch = gtk4::Switch::new();
                if p.on_toggle {
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
                }
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
                if p.on_change {
                    let dispatch = self.dispatch.clone();
                    let seq = self.seq.clone();
                    let slot = self.slot;
                    handlers.push(scale.connect_value_changed(move |scale| {
                        if let Some(value) = render::number(scale.value()) {
                            emit(
                                &dispatch,
                                slot,
                                key,
                                "onChange",
                                vec![value],
                                Some(bump(&seq, key)),
                                false,
                            );
                        }
                    }));
                }
                scale.upcast()
            }
            Element::Spinner(_) => adw::Spinner::new().upcast(),
            Element::Progress(_) => gtk4::ProgressBar::new().upcast(),
            Element::Separator(_) => gtk4::Separator::new(gtk4::Orientation::Horizontal).upcast(),
            Element::Unsupported(_) => {
                let row = Row::new();
                row.set_property("activatable", false);
                row.set_sensitive(false);
                row.upcast()
            }
            _ => gtk4::Box::new(gtk4::Orientation::Vertical, 0).upcast(),
        };
        (widget, inner, handlers)
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
            }
            Element::SwitchRow(p) => {
                widget.set_property("title", p.title.as_deref());
                widget.set_property("subtitle", p.subtitle.as_deref());
                widget.set_property("lead-icon", p.icon.as_deref());
                widget.set_property("active", p.active);
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
                    label.set_ellipsize(if p.wrap {
                        gtk4::pango::EllipsizeMode::None
                    } else {
                        match render::ellipsize(p.ellipsize) {
                            gtk4::pango::EllipsizeMode::None => gtk4::pango::EllipsizeMode::End,
                            mode => mode,
                        }
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
                        if let Some(text) = &p.text {
                            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                            if let Some(icon) = &p.icon {
                                content.append(&gtk4::Image::from_icon_name(icon));
                            }
                            let label = gtk4::Label::new(Some(text));
                            label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                            label.set_max_width_chars(48);
                            content.append(&label);
                            button.set_child(Some(&content));
                        } else if let Some(icon) = &p.icon {
                            button.set_icon_name(icon);
                        } else {
                            button.set_child(None::<&gtk4::Widget>);
                        }
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
                    if switch.is_active() != p.active {
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
                    ) && scale.value() != render::scale_value(p.value, p.min, p.max)
                    {
                        scale.set_value(render::scale_value(p.value, p.min, p.max));
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
            _ => {}
        }
        for handler in &mounted.handlers {
            widget.unblock_signal(handler);
        }
        mounted.node = node.clone();
    }
}

fn handlers_changed(old: &Element, new: &Element) -> bool {
    match (old, new) {
        (Element::Row(a), Element::Row(b)) => a.on_activate != b.on_activate,
        (Element::SwitchRow(a), Element::SwitchRow(b)) => a.on_toggle != b.on_toggle,
        (Element::Fader(a), Element::Fader(b)) => {
            (a.on_change, a.on_mute) != (b.on_change, b.on_mute)
        }
        (Element::Entry(a), Element::Entry(b)) => {
            (a.on_change, a.on_submit) != (b.on_change, b.on_submit)
        }
        (Element::Button(a), Element::Button(b)) => a.on_click != b.on_click,
        (Element::Switch(a), Element::Switch(b)) => a.on_toggle != b.on_toggle,
        (Element::Scale(a), Element::Scale(b)) => a.on_change != b.on_change,
        _ => false,
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
        Element::Root | Element::Unsupported(_) => None,
    }
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
    fn adding_row_handler_rebuilds_and_emits_once() {
        init();
        let first = tree(vec![node(2, "row", json!({"title":"Open"}), vec![])]);
        let (_shell, mut dom, events) = setup(&first);
        widget(&dom, 1, 2, ElementKind::Row).emit_by_name::<()>("clicked", &[]);
        assert!(events.borrow().is_empty());
        let next = first
            .apply(vec![Op::Set {
                id: 2,
                props: serde_json::from_value(json!({"onActivate":true})).expect("props"),
                seq: None,
            }])
            .expect("set");
        assert!(dom.reconcile(&next, 1));
        widget(&dom, 1, 2, ElementKind::Row).emit_by_name::<()>("clicked", &[]);
        assert_eq!(events.borrow().len(), 1);
        assert_eq!(events.borrow()[0].name, "onActivate");
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
        assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::End);
        let progress = widget(&dom, 1, 3, ElementKind::Progress)
            .downcast::<gtk4::ProgressBar>()
            .expect("progress");
        assert_eq!(progress.ellipsize(), gtk4::pango::EllipsizeMode::End);
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

    #[test]
    #[ignore = "needs a display"]
    fn near_limit_first_open_is_measured() {
        init();
        let mut id = 2;
        let mut sections = Vec::new();
        for section in 0..10 {
            let section_id = id;
            id += 1;
            let mut children = Vec::new();
            for _ in 0..if section < 8 { 199 } else { 198 } {
                children.push(node(id, "label", json!({"text":"Item"}), vec![]));
                id += 1;
            }
            sections.push(node(
                section_id,
                "section",
                json!({"title":"List"}),
                children,
            ));
        }
        let tree = tree(sections);
        let start = std::time::Instant::now();
        let (_shell, dom, _) = setup(&tree);
        let elapsed = start.elapsed();
        assert_eq!(dom.nodes.len(), 1998);
        println!("exec first open at node cap: {elapsed:?}");
    }
}
