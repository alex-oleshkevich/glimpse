mod imp;

use gtk4::{glib, prelude::*};

glib::wrapper! {
    pub struct CameraIndicator(ObjectSubclass<imp::CameraIndicator>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl CameraIndicator {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Show the indicator and apply the active style when `active` is true;
    /// hide entirely (collapsing the panel slot) otherwise.
    pub fn set_active(&self, active: bool) {
        self.set_visible(active);
        if active {
            self.add_css_class("is-active");
        } else {
            self.remove_css_class("is-active");
        }
    }
}

impl Default for CameraIndicator {
    fn default() -> Self {
        Self::new()
    }
}
