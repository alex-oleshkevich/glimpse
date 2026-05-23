mod imp;

use gtk4::{glib, prelude::*};
use relm4::{ContainerChild, RelmContainerExt};

glib::wrapper! {
    pub struct Container(ObjectSubclass<imp::Container>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl Container {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerChild for Container {
    type Child = gtk4::Widget;
}

impl RelmContainerExt for Container {
    fn container_add(&self, widget: &impl AsRef<gtk4::Widget>) {
        self.append(widget.as_ref());
    }
}
