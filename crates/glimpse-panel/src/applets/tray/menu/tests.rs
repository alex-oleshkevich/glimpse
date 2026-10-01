use super::*;

fn node(id: i32, label: &str) -> MenuNode {
    MenuNode {
        id,
        label: label.to_owned(),
        enabled: true,
        visible: true,
        disposition: "normal".to_owned(),
        ..Default::default()
    }
}

fn root(children: Vec<MenuNode>) -> MenuNode {
    MenuNode {
        id: 0,
        enabled: true,
        visible: true,
        submenu: true,
        children,
        ..Default::default()
    }
}

fn labels(sections: &[Section]) -> Vec<Vec<String>> {
    sections
        .iter()
        .map(|section| section.0.iter().map(|item| item.label.clone()).collect())
        .collect()
}

#[test]
fn a_separator_becomes_the_boundary_between_two_sections() {
    let mut separator = node(2, "");
    separator.separator = true;
    let layout = root(vec![node(1, "Open"), separator, node(3, "Quit")]);

    assert_eq!(
        labels(&sections(&layout)),
        [vec!["Open".to_owned()], vec!["Quit".to_owned()]],
        "dbusmenu says separator-as-item; GMenu says section boundary"
    );
}

#[test]
fn leading_and_trailing_separators_make_no_empty_sections() {
    let mut first = node(1, "");
    first.separator = true;
    let mut last = node(3, "");
    last.separator = true;
    let layout = root(vec![first, node(2, "Only"), last]);

    assert_eq!(labels(&sections(&layout)), [vec!["Only".to_owned()]]);
}

#[test]
fn two_separators_in_a_row_do_not_produce_a_gap() {
    let mut one = node(2, "");
    one.separator = true;
    let mut two = node(3, "");
    two.separator = true;
    let layout = root(vec![node(1, "A"), one, two, node(4, "B")]);

    assert_eq!(
        labels(&sections(&layout)),
        [vec!["A".to_owned()], vec!["B".to_owned()]]
    );
}

#[test]
fn an_invisible_item_is_not_built_rather_than_built_and_disabled() {
    let mut hidden = node(2, "Never shown");
    hidden.visible = false;
    let layout = root(vec![node(1, "Shown"), hidden]);

    assert_eq!(labels(&sections(&layout)), [vec!["Shown".to_owned()]]);
}

#[test]
fn a_disabled_item_is_built_and_carries_its_state() {
    let mut disabled = node(1, "Resolve conflicts");
    disabled.enabled = false;
    let layout = root(vec![disabled]);

    let built = sections(&layout);
    assert!(!built[0].0[0].enabled);
    assert_eq!(built[0].0[0].action(), "tray.1");
}

#[test]
fn a_checkmark_and_a_radio_keep_their_kind_and_their_state() {
    let mut check = node(1, "Pause syncing");
    check.toggle = MenuToggle::Checkmark;
    check.toggle_state = ToggleState::On;
    let mut radio = node(2, "High");
    radio.toggle = MenuToggle::Radio;
    radio.toggle_state = ToggleState::Off;
    let mut unknown = node(3, "Indeterminate");
    unknown.toggle = MenuToggle::Checkmark;

    let built = sections(&root(vec![check, radio, unknown]));
    let items = &built[0].0;
    assert_eq!(
        (items[0].toggle, items[0].state),
        (MenuToggle::Checkmark, ToggleState::On)
    );
    assert_eq!(
        (items[1].toggle, items[1].state),
        (MenuToggle::Radio, ToggleState::Off)
    );
    assert_eq!(
        items[2].state,
        ToggleState::Indeterminate,
        "no toggle-state is neither on nor off, and must not collapse to off"
    );
}

#[test]
fn a_submenu_recurses_and_keeps_its_own_sections() {
    let mut inner_separator = node(11, "");
    inner_separator.separator = true;
    let mut nested = node(1, "Recent");
    nested.submenu = true;
    nested.children = vec![node(10, "One"), inner_separator, node(12, "Two")];
    let mut deeper = node(20, "Archive");
    deeper.submenu = true;
    deeper.children = vec![node(21, "2025")];
    nested.children.push(deeper);

    let built = sections(&root(vec![nested]));
    let recent = &built[0].0[0];
    assert_eq!(
        labels(&recent.submenu),
        [
            vec!["One".to_owned()],
            vec!["Two".to_owned(), "Archive".to_owned()]
        ]
    );
    let archive = &recent.submenu[1].0[1];
    assert_eq!(labels(&archive.submenu), [vec!["2025".to_owned()]]);
}

#[test]
fn an_item_with_children_but_no_submenu_flag_is_still_a_submenu() {
    let mut parent = node(1, "Has children");
    parent.children = vec![node(2, "Child")];
    let built = sections(&root(vec![parent]));
    assert_eq!(
        labels(&built[0].0[0].submenu),
        [vec!["Child".to_owned()]],
        "children-display is advisory; children are the fact"
    );
}

#[test]
fn a_leaf_has_no_submenu_at_all() {
    let built = sections(&root(vec![node(1, "Quit")]));
    assert!(built[0].0[0].submenu.is_empty());
}

#[test]
fn an_empty_menu_builds_no_sections_rather_than_one_empty_one() {
    assert!(sections(&root(Vec::new())).is_empty());
}
