mod imp;

use glib::Object;
use gtk::glib;

glib::wrapper! {
    pub struct SearchRowObject(ObjectSubclass<imp::SearchRowObject>);
}

impl SearchRowObject {
    pub fn new(id: String, title: String, subtitle: String, icon: String) -> Self {
        Object::builder()
            .property("id", id)
            .property("title", title)
            .property("subtitle", subtitle)
            .property("icon", icon)
            .build()
    }
}

#[derive(Default)]
pub struct SearchRowObjectData {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
}
