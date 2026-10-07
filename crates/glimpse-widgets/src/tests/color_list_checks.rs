use super::*;

pub(super) fn check() {
    let shade = |id: u64, title: &str| Shade {
        id,
        color: rgba([id as u8, 0, 0]),
        title: title.to_owned(),
        notations: vec![
            Notation {
                key: "hex".to_owned(),
                label: "HEX".to_owned(),
                value: title.to_owned(),
            },
            Notation {
                key: "rgb".to_owned(),
                label: "RGB".to_owned(),
                value: format!("rgb({id} 0 0)"),
            },
        ],
    };
    let colors = ColorList::new();
    colors.set_shades(&[shade(7, "#070000"), shade(9, "#090000")]);
    assert_eq!(colors.imp().holders.borrow().len(), 2);
    let color_activated = Rc::new(Cell::new(None));
    colors.connect_activated({
        let color_activated = Rc::clone(&color_activated);
        move |_, id| color_activated.set(Some(id))
    });
    let second = colors.imp().holders.borrow()[1].1.clone();
    assert!(second.details().first_child().is_none());
    second.set_open(true);
    assert!(second.open(), "the row opens its own shade");
    let card = second.details();
    assert!(
        card.first_child().is_some(),
        "the notations are built on first open"
    );
    children_of::<ActionRow>(&card)
        .pop()
        .expect("a copy row")
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(color_activated.get(), Some(9));
    assert_eq!(
        card.first_child()
            .and_downcast::<Swatch>()
            .and_then(|strip| strip.color()),
        Some(rgba([9, 0, 0])),
        "the card opens on a strip of the color itself, wide enough to judge"
    );
    let notation = children_of::<ActionRow>(&card)[1].clone();
    assert_eq!(
        notation.upcast_ref::<Row>().title().as_deref(),
        Some("rgb(9 0 0)")
    );
    let color_copied = Rc::new(RefCell::new(None));
    colors.connect_copied({
        let color_copied = Rc::clone(&color_copied);
        move |_, id, key| *color_copied.borrow_mut() = Some((id, key.to_owned()))
    });
    notation.emit_by_name::<()>("activated", &[]);
    assert_eq!(*color_copied.borrow(), Some((9, "rgb".to_owned())));
    colors.set_shades(&[
        shade(3, "#030000"),
        shade(7, "#070000"),
        shade(9, "#090000"),
    ]);
    assert_eq!(
        colors.imp().holders.borrow()[2].1,
        second,
        "a new color arriving above keeps the open one's row, so its card follows the shade"
    );
    assert!(second.open() && second.details().first_child().is_some());
    assert!(
        colors.imp().holders.borrow()[0]
            .1
            .details()
            .first_child()
            .is_none(),
        "a shade nobody opened carries no card"
    );

    colors.set_shades(&[shade(7, "#070000")]);
    assert_eq!(colors.imp().holders.borrow().len(), 1);
    assert!(!colors.imp().more.get_visible());

    let many: Vec<Shade> = (1..=10)
        .map(|id| shade(id, &format!("#{id:02}0000")))
        .collect();
    colors.set_shades(&many);
    assert_eq!(
        colors.imp().holders.borrow().len(),
        8,
        "a long palette folds past eight rows"
    );
    assert!(colors.imp().more.get_visible());
    assert_eq!(
        colors.imp().more.upcast_ref::<Row>().title().as_deref(),
        Some("2 more colors")
    );
    colors.imp().more.emit_by_name::<()>("activated", &[]);
    assert_eq!(
        colors.imp().holders.borrow().len(),
        10,
        "the fold opens in place"
    );
    assert!(!colors.imp().more.get_visible());
    colors.set_shades(&[shade(7, "#070000")]);

    let picker = ColorPickerPopover::new();
    picker.set_shades(&[]);
    assert!(
        !picker.imp().palette_section.get_visible(),
        "an empty palette is not a section with a placeholder in it"
    );
    assert_eq!(
        picker.imp().shell.subtitle().as_deref(),
        Some("Nothing picked yet"),
        "the header says there is nothing yet"
    );
    let pick_requested = Rc::new(Cell::new(0));
    picker.connect_pick_requested({
        let pick_requested = Rc::clone(&pick_requested);
        move |_| pick_requested.set(pick_requested.get() + 1)
    });
    picker.imp().pick.emit_by_name::<()>("activated", &[]);
    assert_eq!(pick_requested.get(), 1);
    picker.set_latest(None);
    assert!(!picker.imp().latest.get_visible());
    let resting = picker.imp().shell.title();
    picker.set_latest(Some(("#070000", rgba([7, 0, 0]))));
    assert!(picker.imp().latest.get_visible());
    assert_eq!(picker.imp().shell.title().as_deref(), Some("#070000"));
    picker.set_latest(None);
    assert_eq!(picker.imp().shell.title(), resting);
    picker.set_latest(Some(("#070000", rgba([7, 0, 0]))));
    picker.set_shades(&[shade(7, "#070000")]);
    assert!(picker.imp().palette_section.get_visible());
    assert_eq!(picker.imp().shell.subtitle(), None);
}
