mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct SliderRow(ObjectSubclass<imp::SliderRow>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SliderRow {
    fn default() -> Self {
        Self::new()
    }
}

impl SliderRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_start(&self, widget: &impl IsA<gtk4::Widget>) {
        let start = &self.imp().start;
        crate::fill_slot(start, widget);
        start.set_visible(true);
    }

    pub fn connect_moved<F: Fn(&Self, f64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "moved",
            false,
            glib::closure_local!(move |row: Self, value: f64| f(&row, value)),
        )
    }

    pub fn connect_changed<F: Fn(&Self, f64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            false,
            glib::closure_local!(move |row: Self, value: f64| f(&row, value)),
        )
    }
}
