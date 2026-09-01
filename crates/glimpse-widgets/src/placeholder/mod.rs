mod imp;

use gtk4::glib;

glib::wrapper! {
    pub struct Placeholder(ObjectSubclass<imp::Placeholder>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Placeholder {
    fn default() -> Self {
        Self::new()
    }
}

impl Placeholder {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
