mod imp;
mod row;

use gtk4::{glib, subclass::prelude::*};

use crate::services::clock::WorldClockTime;

glib::wrapper! {
    pub struct WorldClock(ObjectSubclass<imp::WorldClock>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl WorldClock {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_rows(&self, rows: &[WorldClockTime]) {
        self.imp().set_rows(rows);
    }
}

impl Default for WorldClock {
    fn default() -> Self {
        Self::new()
    }
}
