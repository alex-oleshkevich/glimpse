mod imp;
mod row;

use chrono::NaiveDate;
use gtk4::{glib, subclass::prelude::*};

use crate::services::calendar_events::CalendarEvent;

glib::wrapper! {
    pub struct Events(ObjectSubclass<imp::Events>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl Events {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_data(&self, date: NaiveDate, events: &[CalendarEvent], loading: bool) {
        self.imp().set_data(date, events, loading);
    }

    pub fn tick(&self) {
        self.imp().tick();
    }
}

impl Default for Events {
    fn default() -> Self {
        Self::new()
    }
}
