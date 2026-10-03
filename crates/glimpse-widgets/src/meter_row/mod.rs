mod imp;

use gtk4::glib;

glib::wrapper! {
    pub struct MeterRow(ObjectSubclass<imp::MeterRow>)
        @extends crate::Row, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for MeterRow {
    fn default() -> Self {
        Self::new()
    }
}

impl MeterRow {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
