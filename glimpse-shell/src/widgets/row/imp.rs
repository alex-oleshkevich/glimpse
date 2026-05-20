use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct Row;

#[glib::object_subclass]
impl ObjectSubclass for Row {
    const NAME: &'static str = "GlimpseRow";
    type Type = super::Row;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for Row {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_orientation(gtk4::Orientation::Horizontal);
        obj.set_spacing(4);
        obj.set_hexpand(true);
        obj.add_css_class("row");
    }
}

impl WidgetImpl for Row {}
impl BoxImpl for Row {}
