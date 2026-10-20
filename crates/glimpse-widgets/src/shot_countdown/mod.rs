mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct ShotCountdown(ObjectSubclass<imp::ShotCountdown>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ShotCountdown {
    fn default() -> Self {
        Self::new()
    }
}

impl ShotCountdown {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_seconds(&self, seconds: u32) {
        let imp = self.imp();
        if imp.seconds.get() == seconds {
            return;
        }
        imp.seconds.set(seconds);
        let text = gettext("Capturing in {n}").replace("{n}", &seconds.to_string());
        imp.label.set_text(&text);
    }

    pub fn connect_cancelled<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "cancelled",
            false,
            glib::closure_local!(move |countdown: Self| f(&countdown)),
        )
    }
}
