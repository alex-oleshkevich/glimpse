mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct Panel(ObjectSubclass<imp::Panel>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

const HORIZONTAL_CLASS: &str = "panel--horizontal";
const VERTICAL_CLASS: &str = "panel--vertical";

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn append_to_start(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().start_box.append(widget);
    }

    pub fn append_to_center(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().center_box.append(widget);
    }

    pub fn append_to_end(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().end_box.append(widget);
    }

    pub fn remove_from_start(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().start_box.remove(widget);
    }

    pub fn remove_from_center(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().center_box.remove(widget);
    }

    pub fn remove_from_end(&self, widget: &impl IsA<gtk4::Widget>) {
        self.imp().end_box.remove(widget);
    }

    pub fn set_orientation(&self, orientation: gtk4::Orientation) {
        let imp = self.imp();
        imp.container.set_orientation(orientation);
        for section in [&imp.start_box, &imp.center_box, &imp.end_box] {
            section.set_orientation(orientation);
        }

        let (add, remove) = match orientation {
            gtk4::Orientation::Vertical => (VERTICAL_CLASS, HORIZONTAL_CLASS),
            _ => (HORIZONTAL_CLASS, VERTICAL_CLASS),
        };
        self.remove_css_class(remove);
        self.add_css_class(add);
    }
}
