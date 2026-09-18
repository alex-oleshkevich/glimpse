mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Choice, Fact, Row, drawer, none_if_empty};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub subtitle: String,
    pub icon_name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeLimit {
    pub enabled: bool,
    pub subtitle: String,
}

glib::wrapper! {
    pub struct BatteryPopover(ObjectSubclass<imp::BatteryPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for BatteryPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl BatteryPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_heading(
        &self,
        icon_name: Option<&str>,
        subtitle: Option<&str>,
        percentage: Option<u8>,
    ) {
        let imp = self.imp();
        imp.hero.set_icon_name(icon_name);
        imp.hero.set_subtitle(subtitle);
        match percentage {
            Some(value) => {
                imp.readout.set_value(Some(value.to_string()));
                imp.readout.set_unit(Some("%"));
                if !imp.readout.get_visible() {
                    imp.readout.set_visible(true);
                }
            }
            None => {
                if imp.readout.get_visible() {
                    imp.readout.set_visible(false);
                }
            }
        }
    }

    pub fn set_profiles(&self, choices: &[Choice], selected: Option<u32>) {
        let imp = self.imp();
        let visible = !choices.is_empty();
        if imp.profiles_section.get_visible() != visible {
            imp.profiles_section.set_visible(visible);
        }
        imp.profiles.set_choices(choices);
        imp.profiles.set_selected(selected);
    }

    pub fn set_devices(&self, devices: &[Device]) {
        let imp = self.imp();
        if imp.devices.borrow().as_slice() == devices {
            return;
        }
        *imp.devices.borrow_mut() = devices.to_vec();
        crate::clear_children(&imp.devices_box);
        imp.devices_section.set_visible(!devices.is_empty());
        for device in devices {
            let row = Row::new();
            row.set_title(none_if_empty(&device.name));
            row.set_subtitle(none_if_empty(&device.subtitle));
            row.set_lead_icon(none_if_empty(&device.icon_name));
            row.set_value(none_if_empty(&device.value));
            row.set_activatable(false);
            imp.devices_box.append(&row);
        }
    }

    pub fn set_details(&self, facts: &[Fact], charge_limit: Option<&ChargeLimit>) {
        let imp = self.imp();
        let has_details = !facts.is_empty() || charge_limit.is_some();
        if imp.details_holder.get_visible() != has_details {
            imp.details_holder.set_visible(has_details);
        }
        if !has_details {
            self.close_details();
        }
        imp.facts.set_facts(facts);
        match charge_limit {
            Some(limit) => {
                if !imp.charge_limit.get_visible() {
                    imp.charge_limit.set_visible(true);
                }
                let row = imp.charge_limit.upcast_ref::<Row>();
                if row.subtitle().as_deref() != Some(limit.subtitle.as_str()) {
                    row.set_subtitle(Some(limit.subtitle.as_str()));
                }
                if imp.charge_limit.active() != limit.enabled {
                    imp.charge_limit.set_active(limit.enabled);
                }
            }
            None => {
                if imp.charge_limit.get_visible() {
                    imp.charge_limit.set_visible(false);
                }
            }
        }
        self.sync_dimming();
    }

    pub fn close_details(&self) {
        drawer::set(&self.imp().details_panel, false);
        self.sync_dimming();
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
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

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    fn sync_dimming(&self) {
        let imp = self.imp();
        let open = imp.details_panel.reveals_child();
        crate::set_css_class(&*imp.details_row, drawer::OPEN, open);
        for widget in [
            imp.hero.upcast_ref::<gtk4::Widget>(),
            imp.profiles_section.upcast_ref(),
            imp.devices_section.upcast_ref(),
            imp.footer.upcast_ref(),
            imp.shell.imp().hero_rule.upcast_ref(),
            imp.shell.imp().footer_rule.upcast_ref(),
        ] {
            crate::set_css_class(widget, drawer::RECEDED, open);
        }
    }
}
