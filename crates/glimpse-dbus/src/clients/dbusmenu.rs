use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

pub type MenuLayout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

pub type MenuItemProperties = (i32, HashMap<String, OwnedValue>);

pub type MenuItemPropertiesRemoved = (i32, Vec<String>);

/// One entry of `EventGroup`: id, event id, data, timestamp.
pub type MenuEvent<'a> = (i32, &'a str, Value<'a>, u32);

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

    fn event_group(&self, events: &[MenuEvent<'_>]) -> zbus::Result<Vec<i32>>;

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

const LABEL: usize = 200;
const ICON_NAME: usize = 200;
const DISPOSITION: usize = 32;
const MOST_CHILDREN: usize = 200;
const DEEPEST: usize = 16;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MenuToggle {
    #[default]
    None,
    Checkmark,
    Radio,
}

/// `-1` is the wire's "indeterminate", which is neither on nor off and must not collapse to off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToggleState {
    #[default]
    Indeterminate,
    Off,
    On,
}

impl ToggleState {
    fn parse(value: i32) -> Self {
        match value {
            0 => ToggleState::Off,
            1 => ToggleState::On,
            _ => ToggleState::Indeterminate,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuNode {
    pub id: i32,
    pub separator: bool,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub icon_name: Option<String>,
    pub toggle: MenuToggle,
    pub toggle_state: ToggleState,
    pub submenu: bool,
    pub disposition: String,
    pub children: Vec<MenuNode>,
}

type Properties = HashMap<String, OwnedValue>;

fn text(properties: &Properties, key: &str, cap: usize) -> Option<String> {
    let raw = <&str>::try_from(properties.get(key)?).ok()?;
    let cleaned = glimpse_utils::clean(raw, cap);
    (!cleaned.is_empty()).then_some(cleaned)
}

/// dbusmenu escapes a literal underscore by doubling it, so a plain `strip('_')` turns
/// `__Recent` into `Recent` rather than `_Recent`. Park the doubled pair on a sentinel no label
/// can contain, drop the mnemonics, then put it back.
pub fn strip_mnemonic(label: &str) -> String {
    label
        .replace("__", "\u{0}")
        .replace('_', "")
        .replace('\u{0}', "_")
}

fn decode_node(
    id: i32,
    properties: &Properties,
    children: &[OwnedValue],
    depth: usize,
) -> MenuNode {
    MenuNode {
        id,
        separator: properties
            .get("type")
            .and_then(|value| <&str>::try_from(value).ok())
            .is_some_and(|kind| kind == "separator"),
        label: text(properties, "label", LABEL)
            .map(|label| strip_mnemonic(&label))
            .unwrap_or_default(),
        enabled: properties
            .get("enabled")
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(true),
        visible: properties
            .get("visible")
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(true),
        icon_name: text(properties, "icon-name", ICON_NAME),
        toggle: match properties
            .get("toggle-type")
            .and_then(|value| <&str>::try_from(value).ok())
        {
            Some("checkmark") => MenuToggle::Checkmark,
            Some("radio") => MenuToggle::Radio,
            _ => MenuToggle::None,
        },
        toggle_state: properties
            .get("toggle-state")
            .and_then(|value| i32::try_from(value).ok())
            .map(ToggleState::parse)
            .unwrap_or_default(),
        submenu: properties
            .get("children-display")
            .and_then(|value| <&str>::try_from(value).ok())
            .is_some_and(|display| display == "submenu"),
        disposition: text(properties, "disposition", DISPOSITION)
            .unwrap_or_else(|| "normal".to_owned()),
        children: decode_children(children, depth),
    }
}

/// Children arrive as `av`, each a variant wrapping the same structure, so a decoder has to
/// recurse through untyped values. A child that will not decode costs its row, never the menu.
fn decode_children(children: &[OwnedValue], depth: usize) -> Vec<MenuNode> {
    if depth >= DEEPEST {
        return Vec::new();
    }
    children
        .iter()
        .take(MOST_CHILDREN)
        .filter_map(|child| {
            let (id, properties, grandchildren) = MenuLayout::try_from(child.clone()).ok()?;
            Some(decode_node(id, &properties, &grandchildren, depth + 1))
        })
        .collect()
}

pub fn decode_layout(layout: &MenuLayout) -> MenuNode {
    let (id, properties, children) = layout;
    decode_node(*id, properties, children, 0)
}

#[cfg(test)]
mod tests;
