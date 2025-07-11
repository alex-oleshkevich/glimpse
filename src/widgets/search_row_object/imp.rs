use std::cell::RefCell;

use glib::Properties;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use super::SearchRowObjectData;

#[derive(Properties, Default)]
#[properties(wrapper_type = super::SearchRowObject)]
pub struct SearchRowObject {
    #[property(name = "id", get, set, type = String, member = id)]
    #[property(name = "title", get, set, type = String, member = title)]
    #[property(name = "subtitle", get, set, type = String, member = subtitle)]
    #[property(name = "icon", get, set, type = String, member = icon)]
    pub data: RefCell<SearchRowObjectData>,
}

#[glib::object_subclass]
impl ObjectSubclass for SearchRowObject {
    const NAME: &'static str = "SearchRowObject";
    type Type = super::SearchRowObject;
}

// Trait shared by all GObjects
#[glib::derived_properties]
impl ObjectImpl for SearchRowObject {}
