mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};
use relm4::{ContainerChild, RelmContainerExt};

glib::wrapper! {
    pub struct PopoverShell(ObjectSubclass<imp::PopoverShell>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl PopoverShell {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn content(&self) -> &gtk4::Box {
        &self.imp().content
    }

    pub fn footer(&self) -> &gtk4::Box {
        &self.imp().footer
    }

    pub fn set_footer_visible(&self, visible: bool) {
        self.imp().footer.set_visible(visible);
    }
}

impl Default for PopoverShell {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerChild for PopoverShell {
    type Child = gtk4::Widget;
}

impl RelmContainerExt for PopoverShell {
    fn container_add(&self, widget: &impl AsRef<gtk4::Widget>) {
        self.imp().content.append(widget.as_ref());
    }
}
