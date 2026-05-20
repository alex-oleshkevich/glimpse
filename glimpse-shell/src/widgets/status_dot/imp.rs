use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct StatusDot;

#[glib::object_subclass]
impl ObjectSubclass for StatusDot {
    const NAME: &'static str = "StatusDot";
    type Type = super::StatusDot;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for StatusDot {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("status-dot");
        obj.add_css_class("neutral");
    }
}

impl WidgetImpl for StatusDot {}
impl BoxImpl for StatusDot {}
