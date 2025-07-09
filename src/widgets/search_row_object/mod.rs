mod imp;

use glib::Object;
use gtk::glib;

glib::wrapper! {
    pub struct SearchRowObject(ObjectSubclass<imp::SearchRowObject>);
}

impl SearchRowObject {
    pub fn new(title: String, subtitle: String, icon: String) -> Self {
        Object::builder()
            .property("title", title)
            .property("subtitle", subtitle)
            .property("icon", icon)
            .build()
    }
}

#[derive(Default)]
pub struct SearchRowObjectData {
    pub title: String,
    pub subtitle: String,
    pub icon: String,
}
