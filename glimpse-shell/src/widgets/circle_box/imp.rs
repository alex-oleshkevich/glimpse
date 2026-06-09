use std::cell::RefCell;

use gtk4::{CssProvider, glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct CircleBox {
    pub(super) provider: RefCell<Option<CssProvider>>,
}

#[glib::object_subclass]
impl ObjectSubclass for CircleBox {
    const NAME: &'static str = "GlimpseCircleBox";
    type Type = super::CircleBox;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for CircleBox {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("circle-box");
        obj.set_orientation(gtk4::Orientation::Vertical);
        obj.set_valign(gtk4::Align::Center);
    }
}

impl WidgetImpl for CircleBox {}
impl BoxImpl for CircleBox {}
