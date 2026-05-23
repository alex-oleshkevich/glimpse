mod imp;

use std::collections::HashSet;

use chrono::NaiveDate;
use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::services::calendar_events::MonthKey;

glib::wrapper! {
    pub struct MonthView(ObjectSubclass<imp::MonthView>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl MonthView {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_visible_month(&self, month: MonthKey) {
        self.imp().set_visible_month(month);
    }

    pub fn set_selected_date(&self, date: NaiveDate) {
        self.imp().set_selected_date(date);
    }

    pub fn selected_date(&self) -> NaiveDate {
        self.imp().selected_date.get()
    }

    pub fn set_event_days(&self, dates: &HashSet<NaiveDate>) {
        self.imp().set_event_days(dates);
    }

    pub fn set_show_week_numbers(&self, show: bool) {
        self.imp().set_show_week_numbers(show);
    }

    pub fn connect_day_selected(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("day-selected", false, closure_local!(move |s: &Self| f(s)))
    }
}

impl Default for MonthView {
    fn default() -> Self {
        Self::new()
    }
}
