mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::Row;

glib::wrapper! {
    pub struct SplitRow(ObjectSubclass<imp::SplitRow>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SplitRow {
    fn default() -> Self {
        Self::new()
    }
}

impl SplitRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn row(&self) -> Row {
        self.imp().row.get()
    }

    pub fn detail(&self) -> gtk4::Button {
        self.imp().detail.get()
    }

    pub fn set_lead(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().row.set_lead(widget);
    }

    pub fn set_trail(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().row.set_trail(widget);
    }

    pub fn connect_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |split: Self| f(&split)),
        )
    }

    pub fn connect_details<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "details",
            false,
            glib::closure_local!(move |split: Self| f(&split)),
        )
    }
}
