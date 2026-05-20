use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct ButtonRow;

#[glib::object_subclass]
impl ObjectSubclass for ButtonRow {
    const NAME: &'static str = "ButtonRow";
    type Type = super::ButtonRow;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for ButtonRow {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_orientation(gtk4::Orientation::Horizontal);
        obj.set_spacing(4);
        obj.set_halign(gtk4::Align::Center);
        obj.add_css_class("button-row");
    }
}

impl WidgetImpl for ButtonRow {}
impl BoxImpl for ButtonRow {}
