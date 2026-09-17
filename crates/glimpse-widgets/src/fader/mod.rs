mod imp;

use gtk4::{glib, prelude::*};

pub(crate) const CHANGED: &str = "changed";
pub(crate) const TOGGLED: &str = "toggled";

glib::wrapper! {
    pub struct Fader(ObjectSubclass<imp::Fader>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Fader {
    fn default() -> Self {
        Self::new()
    }
}

impl Fader {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_changed<F: Fn(&Self, f64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            CHANGED,
            false,
            glib::closure_local!(move |fader: Self, value: f64| f(&fader, value)),
        )
    }

    pub fn connect_toggled<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            TOGGLED,
            false,
            glib::closure_local!(move |fader: Self, muted: bool| f(&fader, muted)),
        )
    }
}
