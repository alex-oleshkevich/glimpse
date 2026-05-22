mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct MediaMeta(ObjectSubclass<imp::MediaMeta>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl MediaMeta {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_title(&self, text: &str) {
        self.imp().title.set_text(text);
    }

    pub fn set_subtitle(&self, text: &str) {
        let label = &self.imp().subtitle;
        label.set_text(text);
        label.set_visible(!text.is_empty());
    }

    pub fn connect_activated(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            closure_local!(move |meta: &Self| f(meta)),
        )
    }
}

impl Default for MediaMeta {
    fn default() -> Self {
        Self::new()
    }
}
