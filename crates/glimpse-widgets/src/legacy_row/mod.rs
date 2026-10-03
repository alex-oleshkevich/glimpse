mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{clear_children, fill_slot};

pub trait LegacyRowImpl: gtk4::subclass::prelude::ButtonImpl {}

unsafe impl<T: LegacyRowImpl> gtk4::subclass::prelude::IsSubclassable<T> for LegacyRow {}

glib::wrapper! {
    pub struct LegacyRow(ObjectSubclass<imp::LegacyRow>)
        @extends gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for LegacyRow {
    fn default() -> Self {
        Self::new()
    }
}

impl LegacyRow {
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

    pub fn trail(&self) -> Option<gtk4::Widget> {
        self.imp().trail.first_child()
    }

    pub fn clear_trail(&self) {
        empty(&self.imp().trail);
    }
}

pub fn beside(row: &LegacyRow, end: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let host = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    row.set_hexpand(true);
    let end = end.as_ref();
    end.set_valign(gtk4::Align::Center);
    host.append(row);
    host.append(end);
    host
}

fn fill(slot: &gtk4::Box, widget: &impl IsA<gtk4::Widget>) {
    fill_slot(slot, widget);
    slot.set_visible(true);
}

fn empty(slot: &gtk4::Box) {
    clear_children(slot);
    slot.set_visible(false);
}
