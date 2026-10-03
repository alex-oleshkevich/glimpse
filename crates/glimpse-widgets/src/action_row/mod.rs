mod imp;

use gtk4::glib;
use gtk4::prelude::*;

glib::wrapper! {
    pub struct ActionRow(ObjectSubclass<imp::ActionRow>)
        @extends crate::Row, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ActionRow {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |row: Self| f(&row)),
        )
    }
}
