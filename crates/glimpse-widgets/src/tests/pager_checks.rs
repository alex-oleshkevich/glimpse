use super::*;

pub(super) fn check() {
    let pager = Pager::new();
    assert!(
        !pager.is_visible(),
        "a pager with no slots hides itself rather than reserving width on the bar"
    );

    let slot = |id: u64, label: &str| Slot {
        id,
        label: label.to_owned(),
        ..Slot::default()
    };
    pager.set_slots(&[slot(4, "1"), slot(7, "2"), slot(9, "3")]);
    assert!(pager.is_visible(), "a pager with slots shows itself");
    assert_eq!(
        children_of::<PagerItem>(&pager).len(),
        3,
        "one item per slot, built once and reused"
    );

    pager.set_slots(&[slot(4, "1")]);
    assert_eq!(
        children_of::<PagerItem>(&pager).len(),
        1,
        "a shorter list unparents the items it no longer has data for"
    );

    pager.set_slots(&[slot(21, "1"), slot(22, "2")]);
    let item = &children_of::<PagerItem>(&pager)[0];
    assert!(
        item.observe_controllers().n_items() == 0,
        "a slot takes no click of its own: the strip is one surface, and a button per slot \
             would swallow the primary press the popover opens on"
    );

    let pressed: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    pager.connect_pressed({
        let pressed = Rc::clone(&pressed);
        move |_| *pressed.borrow_mut() += 1
    });
    pager.emit_by_name::<()>("pressed", &[]);
    assert_eq!(*pressed.borrow(), 1);

    assert!(
        pager.anchor().is_none(),
        "a strip nobody has pressed anchors the arrow on itself rather than on a stale item"
    );

    pager.allocate(120, 24, -1, None);
    let items = children_of::<PagerItem>(&pager);
    let second = items[1].compute_bounds(&pager).expect("allocated item");
    assert_eq!(
        pager.item_at(
            f64::from(second.x() + second.width() / 2.0),
            f64::from(second.y() + second.height() / 2.0),
        ),
        Some(items[1].clone()),
        "the arrow points at the workspace that was pressed, so a press resolves to its item"
    );
    assert!(
        pager.item_at(-1.0, -1.0).is_none(),
        "a press that landed on no item anchors nothing rather than the first one"
    );

    let untouched = child_named::<gtk4::Label>(&pager, "pager-item__label");
    untouched.set_text("tampered");
    pager.set_slots(&[slot(21, "1"), slot(22, "renamed")]);
    assert_eq!(
        untouched.text(),
        "tampered",
        "one changed slot rewrites one item; the rest never reach GTK"
    );

    let label = child_named::<gtk4::Label>(&pager, "pager-item__label");
    assert!(
        !label.is_visible(),
        "a dot carries no text, so the label stays out of the measurement"
    );
    pager.set_shape(Shape::Labels);
    assert!(
        label.is_visible(),
        "labels is the shape that shows the label"
    );
    assert!(
        pager.has_css_class("pager--labels") && !pager.has_css_class("pager--dots"),
        "the shape is one class, so a stylesheet never sees both at once"
    );

    let pill = &children_of::<PagerItem>(&pager)[0];
    let (wide, ..) = pill.measure(gtk4::Orientation::Horizontal, -1);
    let (tall, ..) = pill.measure(gtk4::Orientation::Vertical, -1);
    assert_eq!(
        wide, tall,
        "a one-character number sits in a circle, so padding and min-width are chosen together"
    );

    pager.set_slots(&[Slot {
        id: 21,
        label: "1".to_owned(),
        urgent: true,
        focus: Focus::Here,
        ..Slot::default()
    }]);
    let item = &children_of::<PagerItem>(&pager)[0];
    assert!(
        item.has_css_class("pager-item--urgent") && item.has_css_class("pager-item--here"),
        "urgency is drawn on top of focus rather than replacing it"
    );

    let stepped: Rc<RefCell<Vec<(bool, bool)>>> = Rc::new(RefCell::new(Vec::new()));
    pager.connect_stepped({
        let stepped = Rc::clone(&stepped);
        move |_, horizontal, forward| stepped.borrow_mut().push((horizontal, forward))
    });
    pager.emit_by_name::<()>("stepped", &[&true, &false]);
    assert_eq!(
        *stepped.borrow(),
        [(true, false)],
        "the typed wrapper agrees with the declared parameters, which nothing checks at compile time"
    );

    assert!(
        !pager.has_css_class("pager--vertical") && pager.valign() == gtk4::Align::Center,
        "a pager starts on a horizontal bar, centered across it"
    );

    pager.set_orientation(gtk4::Orientation::Vertical);
    assert!(
        pager.has_css_class("pager--vertical"),
        "the strip has to say which way it runs, because the active dot lengthens along it \
             and a rule keyed on width alone widens the whole column instead"
    );
    assert_eq!(
        pager.halign(),
        gtk4::Align::Center,
        "a vertical strip that fills the bar's width stretches every dot into a bar of its own"
    );

    pager.set_orientation(gtk4::Orientation::Horizontal);
    assert!(
        !pager.has_css_class("pager--vertical") && pager.valign() == gtk4::Align::Center,
        "and back, since a panel's position is a setting that changes under a running applet"
    );
}
