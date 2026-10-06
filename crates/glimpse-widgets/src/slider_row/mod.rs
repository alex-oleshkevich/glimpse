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

    pub(crate) fn set_lead_icon(&self, name: &str) {
        if self.lead_icon().as_deref() == Some(name) {
            return;
        }
        let image = gtk4::Image::from_icon_name(name);
        image.set_accessible_role(gtk4::AccessibleRole::Presentation);
        self.set_start(&image);
    }

    pub(crate) fn lead_icon(&self) -> Option<glib::GString> {
        self.imp()
            .start
            .first_child()
            .and_downcast::<gtk4::Image>()
            .and_then(|image| image.icon_name())
    }

    pub(crate) fn set_range(&self, min: f64, max: f64) {
        self.set_min(min);
        self.set_max(max);
        self.set_step((max / 100.0).max(1.0));
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
