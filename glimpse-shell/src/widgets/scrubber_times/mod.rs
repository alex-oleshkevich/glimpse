mod imp;

use gtk4::{glib, subclass::prelude::*};

glib::wrapper! {
    pub struct ScrubberTimes(ObjectSubclass<imp::ScrubberTimes>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl ScrubberTimes {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_position_text(&self, text: &str) {
        self.imp().position.set_text(text);
    }

    pub fn set_length_text(&self, text: &str) {
        self.imp().length.set_text(text);
    }
}

impl Default for ScrubberTimes {
    fn default() -> Self {
        Self::new()
    }
}
