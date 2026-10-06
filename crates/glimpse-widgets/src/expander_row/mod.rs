mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct ExpanderRow(ObjectSubclass<imp::ExpanderRow>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ExpanderRow {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpanderRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_end(&self, widget: &impl IsA<gtk4::Widget>) {
        if widget.as_ref().parent().is_some()
            && widget.as_ref().parent().as_ref()
                != Some(
                    self.imp()
                        .head
                        .upcast_ref::<crate::Row>()
                        .imp()
                        .end()
                        .upcast_ref(),
                )
        {
            widget.as_ref().unparent();
        }
        self.imp().head.upcast_ref::<crate::Row>().set_end(widget);
    }

    pub fn set_start(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().head.upcast_ref::<crate::Row>().set_start(widget);
    }

    pub fn clear_start(&self) {
        self.imp().head.upcast_ref::<crate::Row>().clear_start();
    }

    pub fn clear_end(&self) {
        self.imp().head.upcast_ref::<crate::Row>().clear_end();
    }

    pub(crate) fn set_openable(&self, openable: bool) {
        self.imp().expandable.set_openable(openable);
        self.imp()
            .head
            .upcast_ref::<crate::Row>()
            .set_chevron(openable);
    }

    pub fn details(&self) -> gtk4::Box {
        self.imp().details.get()
    }

    pub fn connect_toggled<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            glib::closure_local!(move |row: Self, open: bool| f(&row, open)),
        )
    }
}
