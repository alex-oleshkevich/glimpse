mod imp;

use gtk4::glib;

glib::wrapper! {
    pub struct Readout(ObjectSubclass<imp::Readout>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Readout {
    fn default() -> Self {
        Self::new()
    }
}

impl Readout {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
