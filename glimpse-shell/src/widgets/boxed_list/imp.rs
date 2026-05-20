use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct BoxedList;

#[glib::object_subclass]
impl ObjectSubclass for BoxedList {
    const NAME: &'static str = "GlimpseBoxedList";
    type Type = super::BoxedList;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for BoxedList {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_orientation(gtk4::Orientation::Vertical);
        obj.set_overflow(gtk4::Overflow::Hidden);
        obj.add_css_class("boxed-list");
    }
}

impl WidgetImpl for BoxedList {}
impl BoxImpl for BoxedList {}
