mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Clip, ClipActions, Severity, drawer, none_if_empty};

glib::wrapper! {
    pub struct ClipboardPopover(ObjectSubclass<imp::ClipboardPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ClipboardPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_subtitle(&self, subtitle: Option<&str>) {
        self.imp().hero.set_subtitle(subtitle);
    }

    /// The one condition a notification cannot carry: the compositor not offering a data-control
    /// protocol at all. A failed command is reported by a notification and never by a banner here.
    pub fn set_trouble(&self, trouble: Option<&str>) {
        let notice = &self.imp().trouble;
        match trouble {
            Some(reason) => {
                notice.set_severity(Severity::Warning);
                notice.set_title(Some(reason));
                notice.set_visible(true);
            }
            None => notice.set_visible(false),
        }
    }

    pub fn set_actions(&self, actions: ClipActions) {
        self.imp().pinned.set_actions(actions.clone());
        self.imp().recent.set_actions(actions);
    }

    pub fn set_pinned(&self, clips: &[Clip]) {
        self.imp().pinned.set_clips(clips);
        self.imp().pinned_section.set_visible(!clips.is_empty());
    }

    /// The Recent section hides outright when it is empty but something is pinned: a populated
    /// Pinned list above a placeholder reading "Nothing copied yet" contradicts itself.
    pub fn set_recent(&self, clips: &[Clip]) {
        let imp = self.imp();
        imp.recent.set_clips(clips);
        let pinned = imp.pinned_section.get_visible();
        imp.recent_section.set_visible(!clips.is_empty() || !pinned);
        imp.recent_section.set_empty(clips.is_empty());
    }

    /// Which entry has its actions unfolded, across both sections: one list must not keep a panel
    /// open while the other opens a second.
    pub fn set_open(&self, open: Option<u64>) {
        let imp = self.imp();
        imp.pinned.set_open(open);
        imp.recent.set_open(open);
        // Every piece of surrounding chrome recedes while a detail is open, or the card is read
        // against a hero and two footer rows that still look pressable.
        for chrome in [
            imp.hero.upcast_ref::<gtk4::Widget>(),
            imp.trouble.upcast_ref(),
            imp.clear.upcast_ref(),
            imp.footer.upcast_ref(),
        ] {
            crate::set_css_class(chrome, drawer::RECEDED, open.is_some());
        }
    }

    pub fn set_clear_label(&self, label: Option<&str>) {
        let row = &self.imp().clear;
        row.set_title(label);
        row.set_visible(label.is_some());
    }

    pub fn set_footer(&self, label: Option<&str>) {
        let row = &self.imp().footer;
        row.set_title(none_if_empty(label.unwrap_or_default()));
        row.set_visible(label.is_some());
    }

    pub fn connect_restored<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "restored",
            false,
            glib::closure_local!(move |popover: Self, id: u64| f(&popover, id)),
        )
    }

    pub fn connect_detailed<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "detailed",
            false,
            glib::closure_local!(move |popover: Self, id: u64| f(&popover, id)),
        )
    }

    pub fn connect_pinned<F: Fn(&Self, u64, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "pinned",
            false,
            glib::closure_local!(move |popover: Self, id: u64, pinned: bool| f(
                &popover, id, pinned
            )),
        )
    }

    pub fn connect_removed<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "removed",
            false,
            glib::closure_local!(move |popover: Self, id: u64| f(&popover, id)),
        )
    }

    pub fn connect_cleared<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "cleared",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
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
