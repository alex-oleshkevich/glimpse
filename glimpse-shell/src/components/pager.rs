use relm4::gtk::{self, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PagerAppearance {
    #[default]
    Dots,
    Numbers,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PagerItemView {
    pub appearance: PagerAppearance,
    pub label: String,
    pub active: bool,
    pub inactive: bool,
    pub occupied: bool,
    pub urgent: bool,
}

pub fn static_pager_item(view: &PagerItemView) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    root.set_valign(gtk::Align::Center);

    let label = gtk::Label::new(None);
    label.set_valign(gtk::Align::Center);
    label.set_halign(gtk::Align::Center);
    label.set_xalign(0.5);
    label.set_yalign(0.5);
    root.append(&label);

    apply_pager_item_view(&root, &label, view);
    root
}

pub fn static_pager_strip(items: &[PagerItemView]) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    root.add_css_class("pager");
    root.set_valign(gtk::Align::Center);
    for item in items {
        root.append(&static_pager_item(item));
    }
    root
}

pub fn apply_pager_item_view(root: &gtk::Box, label: &gtk::Label, view: &PagerItemView) {
    set_class(root, "pager-dot", view.appearance == PagerAppearance::Dots);
    set_class(
        root,
        "pager-num",
        view.appearance == PagerAppearance::Numbers,
    );
    set_class(root, "active", view.active);
    set_class(root, "inactive", view.inactive);
    set_class(root, "occupied", view.occupied);
    set_class(root, "urgent", view.urgent);
    label.set_visible(view.appearance == PagerAppearance::Numbers);
    label.set_label(&view.label);
}

fn set_class(widget: &gtk::Box, class: &str, active: bool) {
    if active {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}
