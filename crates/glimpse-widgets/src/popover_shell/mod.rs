mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Expandable, SplitRow, clear_children, drawer};

const RECEDES: &str = "recedes";

glib::wrapper! {
    pub struct PopoverShell(ObjectSubclass<imp::PopoverShell>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PopoverShell {
    fn default() -> Self {
        Self::new()
    }
}

impl PopoverShell {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_hero(&self, hero: &impl IsA<gtk4::Widget>) {
        let imp = self.imp();
        clear_children(&imp.hero_box);
        imp.hero_box.append(hero);
        self.watch(hero);
        self.settle();
    }

    pub fn clear_hero(&self) {
        clear_children(&self.imp().hero_box);
        self.settle();
    }

    pub fn set_content(&self, content: &impl IsA<gtk4::Widget>) {
        let content_box = &self.imp().content_box;
        clear_children(content_box);
        content_box.append(content);
        self.watch(content);
        self.settle();
    }

    pub fn clear_content(&self) {
        clear_children(&self.imp().content_box);
        self.settle();
    }

    pub fn append_to_footer(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().footer_box.append(widget);
        self.watch(widget);
        self.settle();
    }

    pub fn clear_footer(&self) {
        clear_children(&self.imp().footer_box);
        self.settle();
    }

    pub fn set_footer_separated(&self, separated: bool) {
        self.imp().footer_rule_suppressed.set(!separated);
        self.settle();
    }

    pub(crate) fn focus(&self, changed: &Expandable) {
        if changed.expanded() {
            for other in descendants::<Expandable>(self.upcast_ref()) {
                let related =
                    &other == changed || other.is_ancestor(changed) || changed.is_ancestor(&other);
                if !related {
                    other.set_expanded(false);
                }
            }
        }
        self.recede(None);
    }

    pub(crate) fn release(&self, gone: &Expandable) {
        self.recede(Some(gone));
    }

    pub(crate) fn dismiss(&self, x: f64, y: f64) -> bool {
        self.pick(x, y, gtk4::PickFlags::DEFAULT)
            .is_some_and(|target| self.dismiss_from(&target))
    }

    pub(crate) fn dismiss_from(&self, target: &gtk4::Widget) -> bool {
        let mut ancestors = std::iter::successors(Some(target.clone()), |widget| widget.parent())
            .take_while(|widget| widget != self.upcast_ref::<gtk4::Widget>());
        let outside = ancestors
            .clone()
            .any(|widget| widget.has_css_class(drawer::RECEDED));
        let opener = ancestors
            .find_map(|widget| widget.downcast::<Expandable>().ok())
            .is_some_and(|expandable| expandable.opens_from(target));
        let outside = outside && !opener;
        if outside {
            for expandable in descendants::<Expandable>(self.upcast_ref()) {
                expandable.set_expanded(false);
            }
        }
        outside
    }

    fn recede(&self, gone: Option<&Expandable>) {
        let open: Vec<Expandable> = descendants::<Expandable>(self.upcast_ref())
            .into_iter()
            .filter(|expandable| expandable.expanded() && Some(expandable) != gone)
            .collect();
        let mut dim = Vec::new();
        if !open.is_empty() {
            for child in children(self.upcast_ref()) {
                outside(&child, &open, &mut dim);
            }
        }
        for widget in descendants::<gtk4::Widget>(self.upcast_ref()) {
            let receded = dim.contains(&widget);
            if receded {
                widget.add_css_class(RECEDES);
            }
            crate::set_css_class(&widget, drawer::RECEDED, receded);
        }
    }

    fn watch(&self, widget: &impl IsA<gtk4::Widget>) {
        widget.as_ref().connect_visible_notify(glib::clone!(
            #[weak(rename_to = shell)]
            self,
            move |_| shell.settle()
        ));
    }

    fn settle(&self) {
        let imp = self.imp();
        let hero_shown = shows_anything(&imp.hero_box);
        imp.hero_box.set_visible(hero_shown);
        imp.hero_rule.set_visible(hero_shown);

        let content_shown = shows_anything(&imp.content_box);
        imp.content_box.set_visible(content_shown);

        let footer_shown = shows_anything(&imp.footer_box);
        imp.footer_box.set_visible(footer_shown);
        imp.footer_rule.set_visible(
            footer_shown
                && !imp.footer_rule_suppressed.get()
                && (content_shown || !hero_shown),
        );
    }
}

fn outside(widget: &gtk4::Widget, open: &[Expandable], dim: &mut Vec<gtk4::Widget>) {
    if widget.is::<gtk4::Scrollbar>() || frame(widget) {
        return;
    }
    let opener = widget
        .parent()
        .and_downcast::<Expandable>()
        .filter(|parent| open.contains(parent))
        .is_some_and(|parent| parent.head::<gtk4::Widget>().as_ref() == Some(widget));
    if opener
        || open
            .iter()
            .any(|expandable| expandable.upcast_ref::<gtk4::Widget>() == widget)
    {
        return;
    }
    if holds_frame(widget) || open.iter().any(|expandable| expandable.is_ancestor(widget)) {
        for child in children(widget) {
            outside(&child, open, dim);
        }
        return;
    }
    dim.push(widget.clone());
}

fn frame(widget: &gtk4::Widget) -> bool {
    widget.is::<gtk4::Separator>() || widget.has_css_class("section__header")
}

fn holds_frame(widget: &gtk4::Widget) -> bool {
    !widget.is::<SplitRow>() && children(widget).any(|child| frame(&child) || holds_frame(&child))
}

fn children(widget: &gtk4::Widget) -> impl Iterator<Item = gtk4::Widget> {
    std::iter::successors(widget.first_child(), |child| child.next_sibling())
}

fn descendants<T: IsA<gtk4::Widget>>(widget: &gtk4::Widget) -> Vec<T> {
    let mut found = Vec::new();
    for child in children(widget) {
        if let Some(hit) = child.downcast_ref::<T>() {
            found.push(hit.clone());
        }
        found.extend(descendants::<T>(&child));
    }
    found
}

fn shows_anything(container: &gtk4::Box) -> bool {
    children(container.upcast_ref()).any(|child| child.get_visible())
}
