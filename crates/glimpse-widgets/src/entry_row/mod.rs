mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

pub(crate) const ENTRY_MAX_CHARS: usize = 4096;

glib::wrapper! {
    pub struct EntryRow(ObjectSubclass<imp::EntryRow>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for EntryRow {
    fn default() -> Self {
        Self::new()
    }
}

impl EntryRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_end(&self, widget: &impl IsA<gtk4::Widget>) {
        let end = &self.imp().end;
        crate::fill_slot(end, widget);
        if !end.get_visible() {
            end.set_visible(true);
        }
    }

    pub fn clear_end(&self) {
        let end = &self.imp().end;
        crate::clear_children(end);
        if end.get_visible() {
            end.set_visible(false);
        }
    }

    pub fn connect_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            false,
            glib::closure_local!(move |row: Self| f(&row)),
        )
    }

    pub fn connect_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |row: Self| f(&row)),
        )
    }
}
