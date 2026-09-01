mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::clear_children;

glib::wrapper! {
    pub struct Section(ObjectSubclass<imp::Section>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Section {
    fn default() -> Self {
        Self::new()
    }
}

impl Section {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_content(&self, content: Option<&impl IsA<gtk4::Widget>>) {
        fill(&self.imp().content, content);
    }

    pub fn set_placeholder(&self, placeholder: Option<&impl IsA<gtk4::Widget>>) {
        fill(&self.imp().placeholder, placeholder);
    }
}

fn fill(slot: &gtk4::Box, widget: Option<&impl IsA<gtk4::Widget>>) {
    clear_children(slot);
    if let Some(widget) = widget {
        slot.append(widget);
    }
}
