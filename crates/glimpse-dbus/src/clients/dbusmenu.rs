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
    /// PNG *file* bytes, not raw pixels.
    pub icon_data: Option<Vec<u8>>,
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
        icon_data: properties
            .get("icon-data")
            .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
            .filter(|bytes| !bytes.is_empty()),
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
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn properties(pairs: &[(&str, Value<'static>)]) -> Properties {
        pairs
            .iter()
            .map(|(key, value)| {
                (
                    (*key).to_owned(),
                    OwnedValue::try_from(value.clone()).expect("a plain value"),
                )
            })
            .collect()
    }

    fn child(id: i32, pairs: &[(&str, Value<'static>)], children: Vec<OwnedValue>) -> OwnedValue {
        OwnedValue::try_from(Value::from((id, properties(pairs), children)))
            .expect("a layout structure")
    }

    #[test]
    fn a_doubled_underscore_survives_the_mnemonic_strip() {
        assert_eq!(strip_mnemonic("_Open __Recent"), "Open _Recent");
        assert_eq!(strip_mnemonic("no mnemonic"), "no mnemonic");
        assert_eq!(strip_mnemonic("____"), "__");
    }

    #[test]
    fn an_absent_property_takes_its_default_rather_than_a_zero_value() {
        let node = decode_node(7, &properties(&[]), &[], 0);
        assert!(node.enabled, "an item with no `enabled` is enabled");
        assert!(node.visible, "an item with no `visible` is visible");
        assert!(!node.separator);
        assert_eq!(node.disposition, "normal");
        assert_eq!(node.toggle, MenuToggle::None);
        assert_eq!(
            node.toggle_state,
            ToggleState::Indeterminate,
            "no toggle-state is indeterminate, which is not off"
        );
        assert!(node.label.is_empty() && node.icon_name.is_none() && node.icon_data.is_none());
    }

    #[test]
    fn a_separator_and_a_checkmark_decode_to_what_they_say_they_are() {
        let separator = decode_node(
            1,
            &properties(&[("type", Value::from("separator"))]),
            &[],
            0,
        );
        assert!(separator.separator);

        let checked = decode_node(
            2,
            &properties(&[
                ("toggle-type", Value::from("checkmark")),
                ("toggle-state", Value::from(1i32)),
            ]),
            &[],
            0,
        );
        assert_eq!(checked.toggle, MenuToggle::Checkmark);
        assert_eq!(checked.toggle_state, ToggleState::On);

        let off = decode_node(
            3,
            &properties(&[
                ("toggle-type", Value::from("radio")),
                ("toggle-state", Value::from(0i32)),
            ]),
            &[],
            0,
        );
        assert_eq!(off.toggle, MenuToggle::Radio);
        assert_eq!(off.toggle_state, ToggleState::Off);
    }

    #[test]
    fn nested_children_decode_through_the_untyped_variants() {
        let deepest = child(3, &[("label", Value::from("2025"))], Vec::new());
        let middle = child(
            2,
            &[
                ("label", Value::from("Archive")),
                ("children-display", Value::from("submenu")),
            ],
            vec![deepest],
        );
        let root = (
            0,
            properties(&[("children-display", Value::from("submenu"))]),
            vec![middle],
        );

        let decoded = decode_layout(&root);
        assert!(decoded.submenu);
        assert_eq!(decoded.children.len(), 1);
        assert_eq!(decoded.children[0].label, "Archive");
        assert_eq!(decoded.children[0].children[0].label, "2025");
    }

    #[test]
    fn a_malformed_child_costs_its_row_and_not_the_menu() {
        let good = child(1, &[("label", Value::from("Quit"))], Vec::new());
        let junk = OwnedValue::try_from(Value::from("not a layout at all")).expect("a string");
        let root = (0, properties(&[]), vec![junk, good]);

        let decoded = decode_layout(&root);
        assert_eq!(decoded.children.len(), 1);
        assert_eq!(decoded.children[0].label, "Quit");
    }

    #[test]
    fn recursion_stops_before_a_cycle_can_exhaust_the_stack() {
        let mut node = child(0, &[("label", Value::from("leaf"))], Vec::new());
        for depth in 1..40 {
            node = child(depth, &[("label", Value::from("deep"))], vec![node]);
        }
        let root = (0, properties(&[]), vec![node]);

        let mut decoded = &decode_layout(&root);
        let mut depth = 0;
        while let Some(first) = decoded.children.first() {
            decoded = first;
            depth += 1;
        }
        assert!(depth <= DEEPEST, "a hostile menu cannot recurse forever");
    }
}
