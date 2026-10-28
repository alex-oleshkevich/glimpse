mod imp;

use gettextrs::{gettext, ngettext};
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Indicator, IndicatorSpec, reconcile::by_key};

const APP_MAX_CHARS: usize = 64;

#[derive(Debug, Clone)]
pub struct ChipGroup {
    pub key: String,
    pub app: String,
    pub icon: Option<gio::Icon>,
    pub count: u32,
}

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

    pub fn set_notifications(&self, groups: &[ChipGroup]) {
        let visible: Vec<ChipGroup> = groups
            .iter()
            .filter(|group| group.count > 0)
            .cloned()
            .collect();
        by_key(
            self,
            &mut self.imp().chips.borrow_mut(),
            &visible,
            |group| group.key.clone(),
            |_| build_chip(),
            apply_chip,
        );
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

pub(crate) fn same_groups(a: &[ChipGroup], b: &[ChipGroup]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.key == b.key
                && a.app == b.app
                && a.count == b.count
                && crate::icons_equal(a.icon.as_ref(), b.icon.as_ref())
        })
}

pub(crate) fn build_chip() -> Indicator {
    glib::Object::builder()
        .property("accessible-role", gtk4::AccessibleRole::Img)
        .build()
}

pub(crate) fn apply_chip(chip: &Indicator, group: &ChipGroup) {
    let name = chip_label(&group.app, group.count);
    chip.apply(&IndicatorSpec {
        icon: group.icon.clone(),
        label: Some(group.count.to_string()),
        tooltip: Some(name.clone()),
        ..Default::default()
    });
    chip.update_property(&[gtk4::accessible::Property::Label(&name)]);
}

pub(crate) fn count_phrase(count: u32) -> String {
    ngettext(
        "{count} new notification",
        "{count} new notifications",
        count,
    )
    .replace("{count}", &count.to_string())
}

fn chip_label(app: &str, count: u32) -> String {
    let app = glimpse_utils::clean(app, APP_MAX_CHARS);
    gettext("{app}: {notifications}")
        .replace("{app}", &app)
        .replace("{notifications}", &count_phrase(count))
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
