mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Choice, Fact, Tick, none_if_empty};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub icon_name: String,
    pub value: String,
    pub warning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeLimit {
    pub enabled: bool,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldNotice {
    pub title: String,
    pub subtitle: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct History {
    pub values: Vec<Option<f64>>,
    pub marked: Vec<bool>,
    pub ticks: Vec<Tick>,
    pub tooltips: Vec<Option<String>>,
}

glib::wrapper! {
    pub struct PowerPopover(ObjectSubclass<imp::PowerPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PowerPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_header(&self, icon_name: &str, percentage: Option<u8>) {
        let imp = self.imp();
        imp.shell.set_icon_name(none_if_empty(icon_name));
        match percentage {
            Some(value) => {
                let value = value.to_string();
                if imp.readout.value().as_deref() != Some(value.as_str()) {
                    imp.readout.set_value(Some(value));
                }
                imp.show_widget(&*imp.readout, true);
            }
            None => imp.show_widget(&*imp.readout, false),
        }
    }

    pub fn set_notice(&self, notice: Option<&HoldNotice>) {
        let imp = self.imp();
        imp.show_widget(&*imp.notice, notice.is_some());
        if let Some(notice) = notice {
            imp.notice.set_title(none_if_empty(&notice.title));
            imp.notice.set_subtitle(none_if_empty(&notice.subtitle));
        }
    }

    pub fn set_remaining(&self, row: Option<(&str, &str)>) {
        let imp = self.imp();
        imp.show_widget(&*imp.remaining, row.is_some());
        if let Some((title, value)) = row {
            imp.remaining.set_title(none_if_empty(title));
            imp.remaining.set_value(none_if_empty(value));
        }
    }

    pub fn set_battery(&self, facts: &[Fact]) {
        let imp = self.imp();
        crate::set_facts(&imp.facts, facts);
        imp.facts_empty.set(facts.is_empty());
        imp.sync_battery();
    }

    pub fn set_charge_limit(&self, limit: Option<&ChargeLimit>) {
        self.imp().apply_limit(limit);
    }

    pub fn set_history(&self, history: Option<&History>) {
        let imp = self.imp();
        if imp.history_data.borrow().as_ref() == history {
            return;
        }
        let next = history.cloned();
        let show = next.is_some();
        if !show && imp.history.open() {
            imp.history.set_open(false);
        }
        imp.show_widget(&*imp.history, show);
        if let Some(history) = next.as_ref() {
            imp.sparkline.set_values(&history.values);
            imp.sparkline.set_marked(&history.marked);
            imp.sparkline.set_ticks(&history.ticks);
            imp.sparkline.set_tooltips(&history.tooltips);
        }
        *imp.history_data.borrow_mut() = next;
    }

    pub fn set_profiles(&self, choices: &[Choice], selected: Option<u32>) {
        let imp = self.imp();
        imp.show_widget(&*imp.profiles_section, !choices.is_empty());
        imp.profiles.set_choices(choices);
        imp.profiles.set_selected(selected);
    }

    pub fn set_hold_active(&self, on: bool) {
        let hold = &self.imp().hold;
        if hold.active() == on {
            return;
        }
        hold.set_active(on);
    }

    pub fn set_keep_awake_visible(&self, visible: bool) {
        self.imp().show_widget(&*self.imp().keep, visible);
    }

    pub fn set_hold_preset(&self, seconds: Option<u32>) {
        crate::apply_hold_preset(self.imp().presets().map(|(row, _)| row), seconds);
    }

    pub fn collapse_hold(&self) {
        self.imp().hold_card.set_open(false);
    }

    pub fn set_devices(&self, devices: &[Device]) {
        let imp = self.imp();
        if imp.devices.borrow().as_slice() == devices {
            return;
        }
        imp.place_devices(devices);
        *imp.devices.borrow_mut() = devices.to_vec();
        imp.show_widget(&*imp.devices_section, !devices.is_empty());
    }

    pub fn connect_profile_activated<F: Fn(&Self, u32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "profile-activated",
            false,
            glib::closure_local!(move |popover: Self, index: u32| f(&popover, index)),
        )
    }

    pub fn connect_charge_limit_toggled<F: Fn(&Self, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "charge-limit-toggled",
            false,
            glib::closure_local!(move |popover: Self, on: bool| f(&popover, on)),
        )
    }

    pub fn connect_hold_toggled<F: Fn(&Self, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "hold-toggled",
            false,
            glib::closure_local!(move |popover: Self, on: bool| f(&popover, on)),
        )
    }

    pub fn connect_hold_requested<F: Fn(&Self, u32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "hold-requested",
            false,
            glib::closure_local!(move |popover: Self, seconds: u32| f(&popover, seconds)),
        )
    }

    pub fn connect_history_opened<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "history-opened",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }
}
