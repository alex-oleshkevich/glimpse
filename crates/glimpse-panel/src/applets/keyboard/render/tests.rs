use super::*;

fn layouts(current: Option<u8>) -> KeyboardLayouts {
    KeyboardLayouts {
        layouts: vec![
            KeyboardLayout {
                code: "US".to_owned(),
                name: "English (US)".to_owned(),
            },
            KeyboardLayout {
                code: "LT".to_owned(),
                name: "Lithuanian".to_owned(),
            },
            KeyboardLayout {
                code: "RU".to_owned(),
                name: "Russian".to_owned(),
            },
        ],
        current,
        restored: false,
    }
}

#[test]
fn hidden_below_two_layouts() {
    assert!(!shown(None));
    assert!(!shown(Some(&KeyboardLayouts {
        layouts: vec![KeyboardLayout {
            code: "US".to_owned(),
            name: "English (US)".to_owned(),
        }],
        current: Some(0),
        restored: false,
    })));
    assert!(shown(Some(&layouts(Some(0)))));
}

#[test]
fn badge_is_the_current_code() {
    assert_eq!(badge(&layouts(Some(2))), Some("RU"));
    assert_eq!(badge(&layouts(None)), Some("US"));
}

#[test]
fn tooltip_defaults_to_the_name() {
    assert_eq!(
        tooltip(&layouts(Some(0)), None).as_deref(),
        Some("English (US)")
    );
    assert_eq!(
        tooltip(&layouts(Some(0)), Some("{code} · {name}")).as_deref(),
        Some("US · English (US)")
    );
}

#[test]
fn rows_flag_the_current_layout() {
    let rows = rows(&layouts(Some(1)));
    assert_eq!(rows.len(), 3);
    assert!(!rows[0].active && rows[1].active && !rows[2].active);
    assert_eq!(rows[1].name, "Lithuanian");
}

#[test]
fn next_and_prev_wrap() {
    assert_eq!(step(&layouts(Some(2)), LayoutRef::Next), Some(0));
    assert_eq!(step(&layouts(Some(0)), LayoutRef::Prev), Some(2));
}

#[test]
fn a_layout_name_that_looks_like_a_placeholder_is_rendered_as_written() {
    let mut layouts = layouts(Some(0));
    layouts.layouts[0].code = "{name}".to_owned();

    assert_eq!(
        tooltip(&layouts, Some("{code} {foo}")).as_deref(),
        Some("{name} {foo}")
    );
}
