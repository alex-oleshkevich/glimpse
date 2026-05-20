mod imp;

use gtk4::{glib, subclass::prelude::*};

glib::wrapper! {
    pub struct Header(ObjectSubclass<imp::Header>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl Header {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_label(&self, text: &str) {
        self.imp().label.set_text(text);
    }
}

impl Default for Header {
    fn default() -> Self {
        Self::new()
    }
}
