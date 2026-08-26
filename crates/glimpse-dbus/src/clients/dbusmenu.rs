use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

pub type MenuLayout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

pub type MenuItemProperties = (i32, HashMap<String, OwnedValue>);

pub type MenuItemPropertiesRemoved = (i32, Vec<String>);

#[zbus::proxy(interface = "com.canonical.dbusmenu", assume_defaults = false)]
pub trait DBusMenu {
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: &[&str],
    ) -> zbus::Result<(u32, MenuLayout)>;

    fn get_group_properties(
        &self,
        ids: &[i32],
        property_names: &[&str],
    ) -> zbus::Result<Vec<MenuItemProperties>>;

    fn get_property(&self, id: i32, name: &str) -> zbus::Result<OwnedValue>;

    fn about_to_show(&self, id: i32) -> zbus::Result<bool>;

    fn about_to_show_group(&self, ids: &[i32]) -> zbus::Result<(Vec<i32>, Vec<i32>)>;

    #[zbus(no_reply)]
    fn event(&self, id: i32, event_id: &str, data: &Value<'_>, timestamp: u32) -> zbus::Result<()>;

    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn text_direction(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn version(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn icon_theme_path(&self) -> zbus::Result<Vec<String>>;

    #[zbus(signal)]
    fn layout_updated(&self, revision: u32, parent_id: i32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn items_properties_updated(
        &self,
        updated: Vec<MenuItemProperties>,
        removed: Vec<MenuItemPropertiesRemoved>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn item_activation_requested(&self, id: i32, timestamp: u32) -> zbus::Result<()>;
}
