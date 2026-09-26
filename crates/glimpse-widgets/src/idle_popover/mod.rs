mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use gettextrs::gettext;

use crate::InhibitorEntry;

glib::wrapper! {
    pub struct IdlePopover(ObjectSubclass<imp::IdlePopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for IdlePopover {
    fn default() -> Self {
        Self::new()
    }
}

impl IdlePopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_heading(&self, icon_name: &str, title: &str, subtitle: Option<&str>) {
        let imp = self.imp();
        imp.hero.set_icon_name(Some(icon_name));
        imp.hero.set_title(Some(title));
        imp.hero.set_subtitle(subtitle);
    }

    pub fn set_hold_active(&self, on: bool) {
        let imp = self.imp();
        if imp.hold.is_active() == on {
            return;
        }
        imp.quiet.set(true);
        imp.hold.set_active(on);
        imp.quiet.set(false);
    }

    /// Other apps' holds, under "Kept awake by"; glimpse's own is the header switch and the hold
    /// row, never a row here.
    pub fn set_inhibitors(&self, entries: &[InhibitorEntry]) {
        let imp = self.imp();
        imp.list.set_inhibitors(entries);
        if imp.others.get_visible() == entries.is_empty() {
            imp.others.set_visible(!entries.is_empty());
        }
        imp.shell.set_footer_separated(!entries.is_empty());
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
    }

    /// The hold row names what a preset will replace — "Awake until 15:40" while a timed hold
    /// runs — and falls back to its own "Keep awake for…" with `None`.
    pub fn set_hold_label(&self, label: Option<&str>) {
        let row = &self.imp().hold_row;
        let wanted = label
            .map(str::to_owned)
            .unwrap_or_else(|| gettext("Keep awake for…"));
        if row.title().as_deref() != Some(wanted.as_str()) {
            row.set_title(Some(wanted.as_str()));
        }
    }

    /// Closes the presets once the hold chosen from them is set.
    pub fn collapse_hold(&self) {
        self.imp().hold_card.set_expanded(false);
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

    pub fn connect_release_requested<F: Fn(&Self, u64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "release-requested",
            false,
            glib::closure_local!(move |popover: Self, id: u64| f(&popover, id)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }
}
