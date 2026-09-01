mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{clear_children, fill_slot};

pub trait RowImpl: gtk4::subclass::prelude::ButtonImpl {}

unsafe impl<T: RowImpl> gtk4::subclass::prelude::IsSubclassable<T> for Row {}

glib::wrapper! {
    pub struct Row(ObjectSubclass<imp::Row>)
        @extends gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Row {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_lead(&self, widget: &impl IsA<gtk4::Widget>) {
        fill(&self.imp().lead, widget);
    }

    pub fn lead(&self) -> Option<gtk4::Widget> {
        self.imp().lead.first_child()
    }

    pub fn clear_lead(&self) {
        empty(&self.imp().lead);
    }

    pub fn set_trail(&self, widget: &impl IsA<gtk4::Widget>) {
        fill(&self.imp().trail, widget);
    }

    pub fn clear_trail(&self) {
        empty(&self.imp().trail);
    }
}

fn fill(slot: &gtk4::Box, widget: &impl IsA<gtk4::Widget>) {
    fill_slot(slot, widget);
    slot.set_visible(true);
}

fn empty(slot: &gtk4::Box) {
    clear_children(slot);
    slot.set_visible(false);
}
