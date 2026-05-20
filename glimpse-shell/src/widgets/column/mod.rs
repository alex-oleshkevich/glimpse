mod imp;

use gtk4::{glib, prelude::*};
use relm4::{ContainerChild, RelmContainerExt};

glib::wrapper! {
    pub struct Column(ObjectSubclass<imp::Column>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl Column {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerChild for Column {
    type Child = gtk4::Widget;
}

impl RelmContainerExt for Column {
    fn container_add(&self, widget: &impl AsRef<gtk4::Widget>) {
        self.append(widget.as_ref());
    }
}
