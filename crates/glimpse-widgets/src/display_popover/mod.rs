mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Choice, Fact, Source, none_if_empty};

pub use imp::NightLight;

const CHANGED: &str = "changed";
const NIGHT_LIGHT_TOGGLED: &str = "night-light-toggled";
const NIGHT_LIGHT_CHANGED: &str = "night-light-changed";
const NIGHT_LIGHT_MOVED: &str = "night-light-moved";
const NIGHT_LIGHT_PAUSED: &str = "night-light-paused";
const NIGHT_LIGHT_SCHEDULED: &str = "night-light-scheduled";
const ENABLE_REQUESTED: &str = "enable-requested";
const BLANKED: &str = "blanked";
const AUTOMATIC: &str = "automatic";
const HOURS: &str = "schedule";
const BRIGHTNESS_ICON: &str = "display-brightness-symbolic";
const DISPLAY_ICON: &str = "video-display-symbolic";

const TEMPERATURE_MIN: f64 = glimpse_config::MIN_TEMPERATURE as f64;
const TEMPERATURE_MAX: f64 = glimpse_config::MAX_TEMPERATURE as f64;

glib::wrapper! {
    pub struct DisplayPopover(ObjectSubclass<imp::DisplayPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for DisplayPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayPopover {
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
        imp.night_light_available.set(snapshot.is_some());
        if let Some(state) = snapshot {
            imp.night_light_state.replace(Some(state.clone()));
        }
        self.render_night_light();
    }

    pub fn forget(&self) {
        let imp = self.imp();
        imp.sources.borrow_mut().clear();
        imp.night_light_state.replace(None);
    }

    pub fn set_displays(&self, displays: &[crate::Display]) {
        let imp = self.imp();
        if imp.entries.borrow().as_slice() == displays {
            return;
        }
        imp.entries.replace(displays.to_vec());
        self.render_displays();
    }

    pub fn set_output_power(&self, supported: bool) {
        let imp = self.imp();
        if imp.output_power.get() == supported {
            return;
        }
        imp.output_power.set(supported);
        self.render_displays();
    }

    pub fn set_blank_screens(&self, supported: bool) {
        let imp = self.imp();
        if imp.blank_screens.get() == supported {
            return;
        }
        imp.blank_screens.set(supported);
        self.render_displays();
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

    pub fn connect_night_light_scheduled<F: Fn(&Self, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            NIGHT_LIGHT_SCHEDULED,
            false,
            glib::closure_local!(move |popover: Self, schedule: String| f(&popover, &schedule)),
        )
    }

    pub fn connect_night_light_paused<F: Fn(&Self, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            NIGHT_LIGHT_PAUSED,
            false,
            glib::closure_local!(move |popover: Self, paused: bool| f(&popover, paused)),
        )
    }

    pub fn connect_enable_requested<F: Fn(&Self, &str, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            ENABLE_REQUESTED,
            false,
            glib::closure_local!(move |popover: Self, connector: String, enabled: bool| f(
                &popover, &connector, enabled
            )),
        )
    }

    pub fn connect_blanked<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            BLANKED,
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

    pub(crate) fn show_temperature(&self, kelvin: f64) {
        let imp = self.imp();
        let kelvin = kelvin_text(kelvin.round().max(0.0) as u32);
        imp.temperature.set_tooltip_text(Some(&kelvin));
        let Some(state) = imp.night_light_state.borrow().clone() else {
            return;
        };
        let schedules = schedules(&state);
        let schedule = (schedules.len() < 2)
            .then(|| {
                schedules
                    .into_iter()
                    .find(|(mode, _)| *mode == state.schedule)
            })
            .flatten()
            .map(|(_, label)| Fact::new(gettext("Schedule"), label));
        let paused = state
            .paused_until
            .as_deref()
            .filter(|time| !time.is_empty())
            .map(|time| Fact::new(gettext("Paused until {time}").replace("{time}", time), ""));
        let facts: Vec<Fact> = schedule
            .into_iter()
            .chain([Fact::new(gettext("Temperature"), kelvin)])
            .chain(paused)
            .collect();
        crate::set_facts(&imp.night_facts, &facts);
    }

    fn render_sources(&self) {
        let imp = self.imp();
        let sources = imp.sources.borrow().clone();
        let primary = sources.first();

        imp.primary.set_visible(primary.is_some());
        imp.readout.set_visible(primary.is_some());
        imp.primary_name.set_visible(sources.len() > 1);

        if let Some(source) = primary {
            imp.primary_name.set_title(none_if_empty(&source.name));
            imp.primary
                .set_lead_icon(none_if_empty(&source.icon).unwrap_or(BRIGHTNESS_ICON));
            imp.primary.set_range(source.floor, source.maximum);
            imp.primary.set_value(source.value);
            let percent = crate::percent_of(imp.primary.value(), source.maximum);
            imp.primary
                .set_tooltip_text(Some(&crate::percent_text(percent)));
            imp.readout.set_value(Some(percent.to_string()));
        }

        let others = sources.get(1..).unwrap_or(&[]);
        imp.devices.set_visible(!others.is_empty());
        imp.devices.set_sources(others);
        self.sync_brightness();
    }

    fn render_night_light(&self) {
        let imp = self.imp();
        let available = imp.night_light_available.get();
        let state = imp.night_light_state.borrow().clone();
        let Some(state) = state else {
            imp.night_light.set_visible(false);
            self.sync_brightness();
            return;
        };

        imp.night_light.set_visible(true);
        imp.night_light.set_sensitive(available);

        if imp.enabled.active() != state.enabled {
            imp.enabled.set_active(state.enabled);
        }
        let problem =
            (state.enabled && !state.serving).then(|| gettext("Not reaching any display"));
        imp.night_light.set_subtitle(problem.as_deref());
        imp.night_light.set_tone(match problem.is_some() {
            true => crate::RowTone::Warning,
            false => crate::RowTone::Normal,
        });

        imp.temperature.set_value(state.temperature as f64);
        self.show_temperature(imp.temperature.value());

        let schedules = schedules(&state);
        let current = schedules
            .iter()
            .position(|(mode, _)| *mode == state.schedule);
        let choices: Vec<Choice> = schedules
            .into_iter()
            .map(|(_, label)| Choice {
                label,
                ..Default::default()
            })
            .collect();
        imp.schedules.set_visible(choices.len() > 1);
        imp.schedules.set_choices(&choices);
        imp.schedules
            .set_selected(current.map(|index| index as u32));
        imp.temperature.set_sensitive(state.enabled);
        self.render_pause();
        self.sync_brightness();
    }

    fn render_pause(&self) {
        let imp = self.imp();
        let (paused, visible) = {
            let state = imp.night_light_state.borrow();
            let paused = state
                .as_ref()
                .is_some_and(|state| state.paused_until.is_some());
            let visible = paused
                || state
                    .as_ref()
                    .is_some_and(|state| state.enabled && state.can_pause);
            (paused, visible)
        };
        imp.pause.set_visible(visible);
        let title = if paused {
            gettext("Resume")
        } else {
            gettext("Pause until tomorrow")
        };
        imp.pause
            .upcast_ref::<crate::Row>()
            .set_title(Some(title.as_str()));
    }

    pub(crate) fn clear_pause(&self) {
        if let Some(state) = self.imp().night_light_state.borrow_mut().as_mut() {
            state.paused_until = None;
        }
        self.render_pause();
    }

    fn request_pause(&self) {
        let imp = self.imp();
        let Some(mut state) = imp.night_light_state.borrow().clone() else {
            return;
        };
        let paused = state.paused_until.is_none();
        if paused && (!state.enabled || !state.can_pause) {
            return;
        }
        state.paused_until = paused.then(String::new);
        if paused {
            state.temperature = glimpse_config::MAX_TEMPERATURE;
        }
        imp.night_light_state.replace(Some(state));
        self.render_night_light();
        self.emit_by_name::<()>(NIGHT_LIGHT_PAUSED, &[&paused]);
    }

    fn render_displays(&self) {
        let imp = self.imp();
        let displays = imp.entries.borrow().clone();
        let power = imp.output_power.get();
        imp.displays_section.set_visible(!displays.is_empty());
        imp.displays.set_output_power(power);
        imp.displays.set_displays(&displays);
        imp.blank
            .set_visible(imp.blank_screens.get() && !displays.is_empty());
    }

    fn sync_brightness(&self) {
        let imp = self.imp();
        let has_sources = !imp.sources.borrow().is_empty();
        let has_night = imp.night_light_state.borrow().is_some();
        imp.brightness.set_visible(has_sources || has_night);
        let icon = match has_sources {
            true => BRIGHTNESS_ICON,
            false => DISPLAY_ICON,
        };
        if imp.shell.icon_name().as_deref() != Some(icon) {
            imp.shell.set_icon_name(Some(icon));
        }
    }

    fn report_night_light_scheduled(&self, index: u32) {
        let schedule = self
            .imp()
            .night_light_state
            .borrow()
            .as_ref()
            .and_then(|state| schedules(state).get(index as usize).map(|(mode, _)| *mode));
        if let Some(schedule) = schedule {
            self.emit_by_name::<()>(NIGHT_LIGHT_SCHEDULED, &[&schedule]);
        }
    }
}

fn schedules(state: &NightLight) -> Vec<(&'static str, String)> {
    let mut schedules = vec![(AUTOMATIC, gettext("Sunset to sunrise"))];
    if state.fixed_hours {
        schedules.push((HOURS, gettext("Fixed hours")));
    }
    schedules
}

fn kelvin_text(kelvin: u32) -> String {
    gettext("{kelvin} K").replace("{kelvin}", &kelvin.to_string())
}
