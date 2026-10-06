mod imp;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

pub use imp::{RowTone, RowTypography};

glib::wrapper! {
    pub struct Row(ObjectSubclass<imp::Row>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
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

    pub fn set_start(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().set_start(widget);
    }

    pub fn clear_start(&self) {
        self.imp().clear_start();
    }

    pub fn set_end(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().set_end(widget);
    }

    pub fn clear_end(&self) {
        crate::clear_children(&self.imp().end());
        self.imp().sync_slots();
        self.imp().sync_accessible();
    }

    pub(crate) fn set_chevron(&self, visible: bool) {
        if self.imp().chevron.get_visible() != visible {
            self.imp().chevron.set_visible(visible);
        }
    }
}

pub trait RowImpl: WidgetImpl {}

unsafe impl<T: RowImpl> IsSubclassable<T> for Row {}
