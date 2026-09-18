mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::Source;

pub use imp::NightLight;

const CHANGED: &str = "changed";
const NIGHT_LIGHT_TOGGLED: &str = "night-light-toggled";
const NIGHT_LIGHT_CHANGED: &str = "night-light-changed";
const NIGHT_LIGHT_MOVED: &str = "night-light-moved";
const FOOTER_ACTIVATED: &str = "footer-activated";

const TEMPERATURE_MIN: f64 = 1000.0;
const TEMPERATURE_MAX: f64 = 6500.0;

glib::wrapper! {
    pub struct BrightnessPopover(ObjectSubclass<imp::BrightnessPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for BrightnessPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl BrightnessPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_sources(&self, sources: &[Source]) {
        let imp = self.imp();
        if imp.sources.borrow().as_slice() == sources {
            return;
        }
        imp.sources.replace(sources.to_vec());
        self.render_sources();
    }

    pub fn set_night_light(&self, snapshot: Option<&NightLight>) {
        let imp = self.imp();
        let becomes_available = snapshot.is_some();
        let unchanged = match snapshot {
            Some(state) => {
                imp.night_light_available.get()
                    && imp.night_light_state.borrow().as_ref() == Some(state)
            }
            None => !imp.night_light_available.get(),
        };
        if unchanged {
            return;
        }

        imp.night_light_available.set(becomes_available);
        if let Some(state) = snapshot {
            imp.night_light_state.replace(Some(state.clone()));
        }
        self.render_night_light();
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_changed<F: Fn(&Self, &str, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            CHANGED,
            false,
            glib::closure_local!(move |popover: Self, key: String, value: f64| f(
                &popover, &key, value
            )),
        )
    }

    pub fn connect_night_light_toggled<F: Fn(&Self, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            NIGHT_LIGHT_TOGGLED,
            false,
            glib::closure_local!(move |popover: Self, on: bool| f(&popover, on)),
        )
    }

    pub fn connect_night_light_changed<F: Fn(&Self, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            NIGHT_LIGHT_CHANGED,
            false,
            glib::closure_local!(move |popover: Self, value: f64| f(&popover, value)),
        )
    }

    pub fn connect_night_light_moved<F: Fn(&Self, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            NIGHT_LIGHT_MOVED,
            false,
            glib::closure_local!(move |popover: Self, value: f64| f(&popover, value)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            FOOTER_ACTIVATED,
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub(crate) fn report_primary_changed(&self, value: f64) {
        let key = self
            .imp()
            .sources
            .borrow()
            .first()
            .map(|source| source.key.clone());
        if let Some(key) = key {
            self.emit_by_name::<()>(CHANGED, &[&key, &value]);
        }
    }

    pub(crate) fn report_night_light_toggled(&self, on: bool) {
        self.imp().temperature.set_visible(on);
        self.emit_by_name::<()>(NIGHT_LIGHT_TOGGLED, &[&on]);
    }

    fn render_sources(&self) {
        let imp = self.imp();
        let sources = imp.sources.borrow().clone();
        let primary = sources.first();

        imp.primary.set_visible(primary.is_some());
        imp.readout.set_visible(primary.is_some());

        if let Some(source) = primary {
            imp.primary.set_maximum(source.maximum);
            imp.primary.set_floor(source.floor);
            imp.primary.set_value(source.value);
            let percent = crate::percent_of(source.value, source.maximum);
            imp.primary
                .set_tooltip_text(Some(&crate::percent_text(percent)));
            imp.readout.set_value(Some(percent.to_string().as_str()));
            imp.readout.set_unit(Some("%"));
        }

        let others = sources.get(1..).unwrap_or(&[]);
        imp.devices.set_visible(!others.is_empty());
        imp.devices.set_sources(others);
    }

    fn render_night_light(&self) {
        let imp = self.imp();
        let available = imp.night_light_available.get();
        let state = imp.night_light_state.borrow().clone();
        let Some(state) = state else {
            imp.night_light.set_visible(false);
            return;
        };

        imp.night_light.set_visible(true);
        imp.night_light.set_sensitive(available);

        if imp.enabled.active() != state.enabled {
            imp.enabled.set_active(state.enabled);
        }

        imp.temperature.set_visible(state.enabled);
        imp.temperature.set_value(state.temperature as f64);
        imp.temperature
            .set_tooltip_text(Some(&kelvin_text(state.temperature)));
    }
}

fn kelvin_text(kelvin: u32) -> String {
    gettext("{kelvin} K").replace("{kelvin}", &kelvin.to_string())
}
