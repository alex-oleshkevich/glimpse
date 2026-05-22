mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct WorldClockRow(ObjectSubclass<imp::WorldClockRow>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl WorldClockRow {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_name(&self, text: &str) {
        self.imp().name.set_label(text);
    }

    pub fn set_day(&self, text: Option<&str>) {
        match text {
            Some(t) => {
                self.imp().day.set_label(&format!("({t})"));
                self.imp().day.set_visible(true);
            }
            None => self.imp().day.set_visible(false),
        }
    }

    pub fn set_time(&self, text: &str) {
        self.imp().time.set_label(text);
    }

    pub fn set_offset(&self, text: &str) {
        self.imp().offset.set_label(text);
    }
}

impl Default for WorldClockRow {
    fn default() -> Self {
        Self::new()
    }
}
