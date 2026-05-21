mod imp;

use gtk4::{glib, subclass::prelude::*};

glib::wrapper! {
    pub struct DateHero(ObjectSubclass<imp::DateHero>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl DateHero {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_weekday(&self, text: &str) {
        self.imp().weekday.set_label(text);
    }

    pub fn set_date(&self, text: &str) {
        self.imp().date.set_label(text);
    }
}

impl Default for DateHero {
    fn default() -> Self {
        Self::new()
    }
}
