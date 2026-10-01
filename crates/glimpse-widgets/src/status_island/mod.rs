mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Indicator, IndicatorSpec};

glib::wrapper! {
    pub struct StatusIsland(ObjectSubclass<imp::StatusIsland>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for StatusIsland {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusIsland {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_weather(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().weather, spec);
    }

    pub fn set_bluetooth(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().bluetooth, spec);
    }

    pub fn set_network(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().network, spec);
    }

    pub fn set_layout(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().layout, spec);
    }

    pub fn set_battery(&self, spec: Option<&IndicatorSpec>) {
        apply_slot(&self.imp().battery, spec);
    }

    pub fn set_session_available(&self, available: bool) {
        let button = &self.imp().power_button;
        if button.get_visible() == available {
            return;
        }
        button.set_visible(available);
    }

    pub fn connect_session_toggled<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "session-toggled",
            false,
            glib::closure_local!(move |island: Self| f(&island)),
        )
    }
}

fn apply_slot(indicator: &Indicator, spec: Option<&IndicatorSpec>) {
    match spec {
        Some(spec) => {
            indicator.apply(spec);
            indicator.set_visible(true);
        }
        None => {
            if indicator.get_visible() {
                indicator.set_visible(false);
            }
        }
    }
}

#[cfg(test)]
mod tests;
