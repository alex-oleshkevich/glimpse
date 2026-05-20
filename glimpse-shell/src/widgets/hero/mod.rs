mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct Hero(ObjectSubclass<imp::Hero>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl Hero {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_icon(&self, name: Option<&str>) {
        self.imp().icon.set_icon_name(name);
    }

    pub fn set_icon_size(&self, size: i32) {
        self.imp().icon.set_pixel_size(size);
    }

    pub fn set_title(&self, text: &str) {
        self.imp().title.set_text(text);
    }

    pub fn set_subtitle(&self, text: &str) {
        self.imp().subtitle.set_text(text);
    }

    pub fn set_trailing_visible(&self, visible: bool) {
        self.imp().trailing.set_visible(visible);
    }

    pub fn set_toggle_active(&self, active: bool) {
        let toggle = &self.imp().toggle;
        if toggle.is_active() != active {
            toggle.set_active(active);
            toggle.set_state(active);
        }
    }

    pub fn set_toggle_sensitive(&self, sensitive: bool) {
        self.imp().toggle.set_sensitive(sensitive);
    }

    pub fn connect_toggled(&self, f: impl Fn(&Self, bool) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            closure_local!(move |hero: &Self, state: bool| f(hero, state)),
        )
    }
}

impl Default for Hero {
    fn default() -> Self {
        Self::new()
    }
}
