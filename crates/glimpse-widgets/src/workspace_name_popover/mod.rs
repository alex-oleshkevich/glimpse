mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::none_if_empty;

glib::wrapper! {
    pub struct WorkspaceNamePopover(ObjectSubclass<imp::WorkspaceNamePopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for WorkspaceNamePopover {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceNamePopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_workspace(&self, title: &str, subtitle: &str) {
        let hero = &self.imp().hero;
        hero.set_title(none_if_empty(title));
        hero.set_subtitle(none_if_empty(subtitle));
    }

    pub fn set_name(&self, name: &str) {
        let imp = self.imp();
        if imp.given.borrow().as_str() == name {
            return;
        }
        let untouched = imp.name.text().as_str() == imp.given.borrow().as_str();
        imp.given.replace(name.to_owned());
        if untouched {
            imp.name.set_text(name);
        }
    }

    pub fn focus_entry(&self) {
        let entry = &self.imp().name;
        entry.grab_focus();
        entry.select_region(0, -1);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_submitted<F: Fn(&Self, String) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "submitted",
            false,
            glib::closure_local!(move |popover: Self, name: String| handler(&popover, name)),
        )
    }

    pub fn connect_cancelled<F: Fn(&Self) + 'static>(&self, handler: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "cancelled",
            false,
            glib::closure_local!(move |popover: Self| handler(&popover)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| handler(&popover)),
        )
    }
}
