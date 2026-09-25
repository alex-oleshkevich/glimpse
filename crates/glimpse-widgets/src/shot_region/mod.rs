mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct ShotRegion(ObjectSubclass<imp::ShotRegion>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ShotRegion {
    fn default() -> Self {
        Self::new()
    }
}

impl ShotRegion {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_selection(&self, selection: Option<(f64, f64, f64, f64)>) {
        let imp = self.imp();
        if *imp.selection.borrow() == selection {
            return;
        }
        imp.selection.replace(selection);
        self.sync_label();
        self.queue_allocate();
        self.queue_draw();
    }

    pub fn set_size_text(&self, text: &str) {
        let imp = self.imp();
        if imp.size_text.borrow().as_str() == text {
            return;
        }
        imp.size_text.replace(text.to_owned());
        imp.size_label.set_text(text);
        self.sync_label();
        self.queue_resize();
    }

    fn sync_label(&self) {
        let imp = self.imp();
        let shown = imp.selection.borrow().is_some() && !imp.size_text.borrow().is_empty();
        if imp.size_label.get_visible() != shown {
            imp.size_label.set_visible(shown);
        }
    }
}
