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
