use std::cell::Cell;
use std::rc::Rc;

use super::*;

pub(super) fn check() {
    let outputs = ChoiceList::new();
    let choice = |label: &str, detail: &str| Choice {
        label: label.to_owned(),
        detail: detail.to_owned(),
        icon_name: "audio-headphones-symbolic".to_owned(),
        warning: false,
    };
    outputs.set_choices(&[
        choice("WH-1000XM5", "Bluetooth"),
        choice("Built-in speakers", ""),
        Choice {
            icon_name: "power-profile-performance-symbolic".into(),
            label: "Performance".into(),
            detail: "Unavailable: laptop on lap".into(),
            warning: true,
        },
    ]);
    let choice_rows = children_of::<ToggleRow>(&outputs);
    assert_eq!(choice_rows.len(), 3);
    assert!(
        choice_rows
            .iter()
            .all(|row| row.control() == ToggleControl::Check)
    );
    assert_eq!(
        face(&choice_rows[0]).icon_name().as_deref(),
        Some("audio-headphones-symbolic")
    );
    assert_eq!(
        face(&choice_rows[0]).subtitle().as_deref(),
        Some("Bluetooth")
    );
    assert_eq!(face(&choice_rows[0]).tone(), RowTone::Normal);
    assert_eq!(face(&choice_rows[1]).subtitle(), None);
    assert_eq!(
        face(&choice_rows[2]).subtitle().as_deref(),
        Some("Unavailable: laptop on lap")
    );
    assert_eq!(face(&choice_rows[2]).tone(), RowTone::Warning);
    assert!(
        !choice_rows[0].active() && !choice_rows[1].active() && !choice_rows[2].active(),
        "a list nobody has chosen from shows no check"
    );

    let seen = Rc::new(Cell::new(0u32));
    outputs.connect_activated({
        let seen = Rc::clone(&seen);
        move |list, index| {
            assert_eq!(list.selected(), Some(index));
            seen.set(seen.get() + 1);
        }
    });
    outputs.set_selected(Some(1));
    assert_eq!(seen.get(), 0);
    assert!(!choice_rows[0].active() && choice_rows[1].active());

    choice_rows[0].activate();
    assert_eq!(seen.get(), 1);
    assert_eq!(outputs.selected(), Some(0));
    assert!(choice_rows[0].active() && !choice_rows[1].active());
    choice_rows[0].activate();
    assert_eq!(seen.get(), 1);
    assert!(choice_rows[0].active() && !choice_rows[1].active() && !choice_rows[2].active());

    outputs.set_choices(&[choice("Built-in speakers", "")]);
    assert_eq!(outputs.selected(), None);
    assert_eq!(children_of::<ToggleRow>(&outputs).len(), 1);

    outputs.set_selected(Some(7));
    assert_eq!(outputs.selected(), None);
}

fn face(row: &ToggleRow) -> &Row {
    row.upcast_ref()
}
