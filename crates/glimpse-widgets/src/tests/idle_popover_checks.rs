use super::*;

pub(super) fn check() {
    let idle = IdlePopover::new();
    let idle_window = gtk4::Window::new();
    idle_window.set_child(Some(&idle));
    let idle_hold = idle.imp().hold.clone();
    let idle_hold_card = idle.imp().hold_card.clone();
    let idle_hold_row = child_named::<ActionRow>(&idle_hold_card, "expander-row__head");
    assert!(
        !idle.imp().others.get_visible(),
        "no other app holding means no \"Kept awake by\" section at all"
    );

    idle.set_heading("preferences-desktop-screensaver-symbolic", "Idle", None);
    idle.set_hold_active(true);
    assert!(idle_hold.is_active());

    let idle_toggled = Rc::new(RefCell::new(Vec::<bool>::new()));
    idle.connect_hold_toggled({
        let idle_toggled = Rc::clone(&idle_toggled);
        move |_, on| idle_toggled.borrow_mut().push(on)
    });
    idle.set_hold_active(false);
    assert!(
        idle_toggled.borrow().is_empty(),
        "dressing the switch programmatically must not report it as a user action"
    );
    idle_hold.set_active(true);
    assert_eq!(*idle_toggled.borrow(), [true]);

    assert!(!idle_hold_card.open(), "the duration choices start closed");
    assert_eq!(idle_hold_card.title().as_deref(), Some("Keep awake for…"));
    idle.set_hold_label(Some("Awake until 15:40"));
    assert_eq!(
        idle_hold_card.title().as_deref(),
        Some("Awake until 15:40"),
        "while a timed hold runs, the row names what a preset would replace"
    );
    idle.set_hold_label(None);
    assert_eq!(idle_hold_card.title().as_deref(), Some("Keep awake for…"));

    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class("receded"))
    };
    idle.set_inhibitors(&[inhibitor(1, "Zoom", true)]);

    idle_hold_row.activate();
    assert!(
        idle_hold_card.open()
            && dimmed(idle.imp().list.upcast_ref()),
        "the duration row opens its choices and dims the rest of the popover"
    );
    idle_hold_row.activate();
    assert!(
        !idle_hold_card.open() && !dimmed(idle.imp().list.upcast_ref()),
        "the same duration row closes its choices and restores the popover"
    );

    let idle_requested = Rc::new(RefCell::new(Vec::<u32>::new()));
    idle.connect_hold_requested({
        let idle_requested = Rc::clone(&idle_requested);
        move |_, seconds| idle_requested.borrow_mut().push(seconds)
    });
    for (row, seconds) in idle.imp().presets() {
        row.activate();
        assert_eq!(
            *idle_requested.borrow().last().unwrap(),
            seconds,
            "each preset row must ask for its own duration, not a neighbour's"
        );
        assert!(
            idle.imp()
                .presets()
                .iter()
                .all(|(other, preset)| other.active() == (*preset == seconds)),
            "the chosen preset alone is checked"
        );
    }
    idle.imp().preset_4h.activate();
    assert!(
        idle.imp().preset_4h.active(),
        "pressing the running preset again keeps it checked"
    );
    idle.set_hold_preset(None);
    assert!(
        idle.imp().presets().iter().all(|(row, _)| !row.active()),
        "no timed hold checks no preset"
    );
    assert_eq!(
        *idle_requested.borrow(),
        [900, 1800, 3600, 7200, 14400, 14400],
        "no preset asks for an indefinite hold: the switch is that"
    );
    idle_hold_row.activate();
    idle.collapse_hold();
    assert!(
        !idle_hold_card.open(),
        "a hold that was set closes the choices it was picked from"
    );

    idle.set_inhibitors(&[inhibitor(1, "Zoom", true)]);
    assert!(idle.imp().others.get_visible());
    let idle_released = Rc::new(RefCell::new(Vec::<u64>::new()));
    idle.connect_release_requested({
        let idle_released = Rc::clone(&idle_released);
        move |_, id| idle_released.borrow_mut().push(id)
    });
    let item = children_of::<ExpanderRow>(&*idle.imp().list).remove(0);
    child_named::<ActionRow>(&item, "expander-row__head").activate();
    assert!(item.open() && dimmed(idle_hold_card.upcast_ref()));
    let release = item
        .details()
        .last_child()
        .and_downcast::<ActionRow>()
        .expect("Release");
    assert_eq!(release.upcast_ref::<Row>().tone(), RowTone::Destructive);
    release.activate();
    assert_eq!(*idle_released.borrow(), [1]);

    idle.set_inhibitors(&[]);
    assert!(
        !idle.imp().others.get_visible() && !dimmed(idle_hold_card.upcast_ref()),
        "the section and the dimming clear when the list empties"
    );

}
