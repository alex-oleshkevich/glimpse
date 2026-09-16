mod imp;

use gtk4::{glib, prelude::*};

pub(crate) const TOGGLED: &str = "toggled";

glib::wrapper! {
    pub struct SwitchRow(ObjectSubclass<imp::SwitchRow>)
        @extends crate::Row, gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SwitchRow {
    fn default() -> Self {
        Self::new()
    }
}

impl SwitchRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_toggled<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            TOGGLED,
            false,
            glib::closure_local!(move |row: Self, on: bool| f(&row, on)),
        )
    }
}
