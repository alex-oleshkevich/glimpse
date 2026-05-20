use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct Column;

#[glib::object_subclass]
impl ObjectSubclass for Column {
    const NAME: &'static str = "GlimpseColumn";
    type Type = super::Column;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for Column {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_orientation(gtk4::Orientation::Vertical);
        obj.set_spacing(4);
        obj.add_css_class("column");
    }
}

impl WidgetImpl for Column {}
impl BoxImpl for Column {}
