mod imp;

use gtk4::{glib, subclass::prelude::*};

glib::wrapper! {
    pub struct EventRow(ObjectSubclass<imp::EventRow>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl EventRow {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_title(&self, text: &str) {
        self.imp().set_title(text);
    }

    pub fn set_time(&self, text: &str) {
        self.imp().set_time(text);
    }
}

impl Default for EventRow {
    fn default() -> Self {
        Self::new()
    }
}
