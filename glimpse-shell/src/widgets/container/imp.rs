use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct Container;

#[glib::object_subclass]
impl ObjectSubclass for Container {
    const NAME: &'static str = "Container";
    type Type = super::Container;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for Container {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("container");
        obj.set_orientation(gtk4::Orientation::Vertical);
    }
}

impl WidgetImpl for Container {}
impl BoxImpl for Container {}
