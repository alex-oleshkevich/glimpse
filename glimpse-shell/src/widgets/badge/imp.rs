use gtk4::{glib, prelude::*, subclass::prelude::*};

pub struct Badge {
    pub(super) label: gtk4::Label,
}

impl Default for Badge {
    fn default() -> Self {
        Self {
            label: gtk4::Label::new(None),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Badge {
    const NAME: &'static str = "Badge";
    type Type = super::Badge;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for Badge {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("badge");
        obj.add_css_class("default");
        obj.append(&self.label);
    }
}

impl WidgetImpl for Badge {}
impl BoxImpl for Badge {}
