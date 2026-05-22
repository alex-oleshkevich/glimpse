mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct CalendarControls(ObjectSubclass<imp::CalendarControls>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl CalendarControls {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_title(&self, text: &str) {
        self.imp().title.set_label(text);
    }

    pub fn connect_prev_clicked(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("prev-clicked", false, closure_local!(move |s: &Self| f(s)))
    }

    pub fn connect_next_clicked(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("next-clicked", false, closure_local!(move |s: &Self| f(s)))
    }

    pub fn connect_today_clicked(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("today-clicked", false, closure_local!(move |s: &Self| f(s)))
    }

    pub fn connect_title_clicked(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("title-clicked", false, closure_local!(move |s: &Self| f(s)))
    }
}

impl Default for CalendarControls {
    fn default() -> Self {
        Self::new()
    }
}
