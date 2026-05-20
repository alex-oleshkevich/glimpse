use gtk4::{glib, prelude::*, subclass::prelude::*};

pub struct Header {
    pub(super) label: gtk4::Label,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            label: gtk4::Label::new(None),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Header {
    const NAME: &'static str = "GlimpseHeader";
    type Type = super::Header;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for Header {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("header");
        self.label.add_css_class("caption-heading");
        self.label.add_css_class("dim-label");
        self.label.set_xalign(0.0);
        obj.append(&self.label);
    }
}

impl WidgetImpl for Header {}
impl BoxImpl for Header {}
