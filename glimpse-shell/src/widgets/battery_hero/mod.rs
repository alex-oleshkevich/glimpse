mod imp;

use gtk4::{glib, subclass::prelude::*};

glib::wrapper! {
    pub struct BatteryHero(ObjectSubclass<imp::BatteryHero>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl BatteryHero {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_icon_name(&self, icon: &str) {
        self.imp().icon.set_icon_name(Some(icon));
    }

    pub fn set_percentage(&self, text: &str) {
        self.imp().percentage.set_label(text);
    }

    pub fn set_fraction(&self, fraction: f64) {
        self.imp().progress.set_fraction(fraction);
    }

    pub fn set_state(&self, text: &str) {
        self.imp().state.set_label(text);
    }
}

impl Default for BatteryHero {
    fn default() -> Self {
        Self::new()
    }
}
