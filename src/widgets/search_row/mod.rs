mod imp;

use glib::Object;
use glib::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

use crate::widgets::search_row_object::SearchRowObject;

glib::wrapper! {
    pub struct SearchRow(ObjectSubclass<imp::SearchRow>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for SearchRow {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchRow {
    pub fn new() -> Self {
        Object::builder().build()
    }

    pub fn bind(&self, search_object: &SearchRowObject) {
        let title_label = self.imp().title.get();
        let subtitle_label = self.imp().subtitle.get();
        let icon_image = self.imp().icon.get();
        let mut bindings = self.imp().bindings.borrow_mut();

        let title_binding = search_object
            .bind_property("title", &title_label, "label")
            .bidirectional()
            .sync_create()
            .build();
        bindings.push(title_binding);

        let subtitle_binding = search_object
            .bind_property("subtitle", &subtitle_label, "label")
            .sync_create()
            .build();
        bindings.push(subtitle_binding);

        let icon_binding = search_object
            .bind_property("icon", &icon_image, "icon-name")
            .sync_create()
            .build();
        bindings.push(icon_binding);
    }

    pub fn unbind(&self) {
        for binding in self.imp().bindings.borrow_mut().drain(..) {
            binding.unbind();
        }
    }
}
