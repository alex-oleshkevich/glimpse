use gtk4::{glib, prelude::*, subclass::prelude::*};

pub struct Text {
    pub(super) label: gtk4::Label,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            label: gtk4::Label::new(None),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Text {
    const NAME: &'static str = "Text";
    type Type = super::Text;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for Text {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("text");
        self.label.set_xalign(0.0);
        self.label.set_hexpand(true);
        obj.append(&self.label);
    }
}

impl WidgetImpl for Text {}
impl BoxImpl for Text {}
