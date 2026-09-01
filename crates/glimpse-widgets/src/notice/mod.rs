mod imp;

use gtk4::glib;

pub use imp::Severity;

glib::wrapper! {
    pub struct Notice(ObjectSubclass<imp::Notice>)
        @extends gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Notice {
    fn default() -> Self {
        Self::new()
    }
}

impl Notice {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
