mod imp;

use gtk4::glib;

glib::wrapper! {
    pub struct WidgetDemoPopover(ObjectSubclass<imp::WidgetDemoPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for WidgetDemoPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetDemoPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
