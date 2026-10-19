mod imp;

use gtk4::glib;
use gtk4::prelude::*;

pub use imp::ToggleControl;

glib::wrapper! {
    pub struct ToggleRow(ObjectSubclass<imp::ToggleRow>)
        @extends crate::Row, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ToggleRow {
    fn default() -> Self {
        Self::new()
    }
}

impl ToggleRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_toggled<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            glib::closure_local!(move |row: Self, active: bool| f(&row, active)),
        )
    }
}
