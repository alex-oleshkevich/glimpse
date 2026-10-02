mod imp;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

pub use imp::Paint;

glib::wrapper! {
    pub struct Container(ObjectSubclass<imp::Container>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Container {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_child(&self, child: Option<&impl IsA<gtk4::Widget>>) {
        let next = child.map(|widget| widget.as_ref().clone());
        let current = self.imp().child.borrow().clone();
        if current == next {
            return;
        }
        if let Some(old) = current {
            old.unparent();
        }
        if let Some(widget) = &next {
            self.imp().place(widget);
            widget.set_parent(self);
        }
        *self.imp().child.borrow_mut() = next;
    }
}
