mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct ChoiceTile(ObjectSubclass<imp::ChoiceTile>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl ChoiceTile {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_primary(&self, text: &str) {
        self.imp().primary_label.set_text(text);
    }

    pub fn set_secondary(&self, text: Option<&str>) {
        let label = &self.imp().secondary_label;
        match text {
            Some(t) => {
                label.set_text(t);
                label.set_visible(true);
            }
            None => label.set_visible(false),
        }
    }

    pub fn set_left(&self, child: Option<impl IsA<gtk4::Widget>>) {
        let slot = &self.imp().left_slot;
        if let Some(w) = slot.first_child() {
            slot.remove(&w);
        }
        if let Some(w) = child {
            slot.append(&w);
            slot.set_visible(true);
        } else {
            slot.set_visible(false);
        }
    }

    pub fn set_selected(&self, selected: bool) {
        self.imp().selected.set(selected);
        self.imp().checkmark.set_visible(selected);
        if selected {
            self.add_css_class("selected");
        } else {
            self.remove_css_class("selected");
        }
    }

    pub fn is_selected(&self) -> bool {
        self.imp().selected.get()
    }

    pub fn connect_activated(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("activated", false, closure_local!(move |tile: &Self| f(tile)))
    }
}

impl Default for ChoiceTile {
    fn default() -> Self {
        Self::new()
    }
}
