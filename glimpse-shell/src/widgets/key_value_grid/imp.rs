use gtk4::{glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;

#[derive(Default)]
pub struct KeyValueGrid {
    pub(super) row_count: Cell<i32>,
}

#[glib::object_subclass]
impl ObjectSubclass for KeyValueGrid {
    const NAME: &'static str = "KeyValueGrid";
    type Type = super::KeyValueGrid;
    type ParentType = gtk4::Grid;
}

impl ObjectImpl for KeyValueGrid {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("key-value-grid");
        obj.set_column_spacing(8);
        obj.set_row_spacing(2);
    }
}

impl WidgetImpl for KeyValueGrid {}
impl GridImpl for KeyValueGrid {}
