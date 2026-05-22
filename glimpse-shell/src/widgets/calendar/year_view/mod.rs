mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct YearView(ObjectSubclass<imp::YearView>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl YearView {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn visible_year(&self) -> i32 {
        self.imp().visible_year.get()
    }

    pub fn set_current_month(&self, year: i32, month: u32) {
        self.imp().set_current_month(year, month);
    }

    pub fn connect_month_picked(&self, f: impl Fn(&Self, u32) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "month-picked",
            false,
            closure_local!(move |s: &Self, month: u32| f(s, month)),
        )
    }
}

impl Default for YearView {
    fn default() -> Self {
        Self::new()
    }
}
