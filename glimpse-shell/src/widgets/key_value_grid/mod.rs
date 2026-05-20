mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct KeyValueGrid(ObjectSubclass<imp::KeyValueGrid>)
        @extends gtk4::Grid, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl KeyValueGrid {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn add_row(&self, key: &str, value: &str) {
        let row = self.imp().row_count.get();

        let key_label = gtk4::Label::new(Some(key));
        key_label.set_xalign(0.0);
        key_label.add_css_class("dim-label");
        key_label.add_css_class("key-value-grid__key");

        let value_label = gtk4::Label::new(Some(value));
        value_label.set_xalign(1.0);
        value_label.set_hexpand(true);
        value_label.add_css_class("numeric");
        value_label.add_css_class("key-value-grid__value");

        self.attach(&key_label, 0, row, 1, 1);
        self.attach(&value_label, 1, row, 1, 1);

        self.imp().row_count.set(row + 1);
    }

    pub fn clear(&self) {
        while let Some(child) = self.first_child() {
            self.remove(&child);
        }
        self.imp().row_count.set(0);
    }
}

impl Default for KeyValueGrid {
    fn default() -> Self {
        Self::new()
    }
}
