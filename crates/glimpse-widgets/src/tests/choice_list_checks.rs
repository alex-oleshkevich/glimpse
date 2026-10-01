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
    ]);
    let choice_rows = children_of::<Row>(&outputs);
    assert_eq!(choice_rows.len(), 2);
    assert_eq!(choice_rows[0].subtitle().as_deref(), Some("Bluetooth"));
    assert_eq!(choice_rows[1].subtitle(), None);
    assert!(
        choice_rows.iter().all(|row| row.selectable()),
        "every row reserves the check, so choosing one does not shunt the labels sideways"
    );
    assert!(
        !choice_rows[0].selected() && !choice_rows[1].selected(),
        "a list nobody has chosen from shows no check at all rather than defaulting to the \
             first, which would claim something untrue about the backend"
    );

    outputs.set_selected(Some(1));
    assert!(!choice_rows[0].selected() && choice_rows[1].selected());

    outputs.connect_activated(|list, index| {
        assert_eq!(
            list.selected(),
            Some(index),
            "the check has already moved by the time the handler runs; a list that waits for \
                 the backend to confirm shows the old row as chosen for a whole round trip"
        );
    });
    choice_rows[0].emit_clicked();
    assert_eq!(outputs.selected(), Some(0));
    assert!(choice_rows[0].selected() && !choice_rows[1].selected());

    outputs.set_choices(&[choice("Built-in speakers", "")]);
    assert_eq!(
        outputs.selected(),
        None,
        "any change to the list drops the selection, because it is positional: index 0 \
             named the headphones a moment ago and names the speakers now, so keeping it would \
             quietly check the wrong device"
    );
    assert_eq!(children_of::<Row>(&outputs).len(), 1);

    outputs.set_selected(Some(7));
    assert_eq!(
        outputs.selected(),
        None,
        "and an index past the end is refused rather than stored to confuse the next render"
    );
}
