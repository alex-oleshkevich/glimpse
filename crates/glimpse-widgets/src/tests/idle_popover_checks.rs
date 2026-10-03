use super::*;

pub(super) fn check() {
    let idle = IdlePopover::new();
    let idle_window = gtk4::Window::new();
    idle_window.set_child(Some(&idle));
    let idle_hold = idle.imp().hold.clone();
    let idle_hold_row = idle.imp().hold_row.clone();
    let idle_hold_card = idle.imp().hold_card.clone();
    let _hold_chevron = child_named::<gtk4::Image>(&idle_hold_row, "drawer-chevron");
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

    assert!(
        !idle_hold_card.expanded(),
        "the duration choices start closed"
    );
    assert_eq!(idle_hold_row.title().as_deref(), Some("Keep awake for…"));
    idle.set_hold_label(Some("Awake until 15:40"));
    assert_eq!(
        idle_hold_row.title().as_deref(),
        Some("Awake until 15:40"),
        "while a timed hold runs, the row names what a preset would replace"
    );
    idle.set_hold_label(None);
    assert_eq!(idle_hold_row.title().as_deref(), Some("Keep awake for…"));

    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class("receded"))
    };
    idle.set_inhibitors(&[inhibitor(1, "Zoom", true)]);
    idle.set_footer(Some("Idle settings"));
    idle_hold_row.emit_clicked();
    assert!(
        idle_hold_card.expanded()
            && dimmed(idle.imp().hero.upcast_ref())
            && dimmed(idle.imp().list.upcast_ref())
            && dimmed(idle.imp().footer.upcast_ref()),
        "the duration row opens its choices and dims the rest of the popover"
    );
    idle_hold_row.emit_clicked();
    assert!(
        !idle_hold_card.expanded() && !dimmed(idle.imp().hero.upcast_ref()),
        "the same duration row closes its choices and restores the popover"
    );

    let idle_requested = Rc::new(RefCell::new(Vec::<u32>::new()));
    idle.connect_hold_requested({
        let idle_requested = Rc::clone(&idle_requested);
        move |_, seconds| idle_requested.borrow_mut().push(seconds)
    });
    for (button, seconds) in [
        (&idle.imp().preset_15m, 900u32),
        (&idle.imp().preset_30m, 1800),
        (&idle.imp().preset_1h, 3600),
        (&idle.imp().preset_2h, 7200),
        (&idle.imp().preset_4h, 14400),
    ] {
        button.emit_clicked();
        assert_eq!(
            *idle_requested.borrow().last().unwrap(),
            seconds,
            "each preset button must ask for its own duration, not a neighbour's"
        );
    }
    assert_eq!(
        *idle_requested.borrow(),
        [900, 1800, 3600, 7200, 14400],
        "no preset asks for an indefinite hold: the switch is that"
    );
    idle_hold_row.emit_clicked();
    idle.collapse_hold();
    assert!(
        !idle_hold_card.expanded(),
        "a hold that was set closes the choices it was picked from"
    );

    idle.set_inhibitors(&[inhibitor(1, "Zoom", true)]);
    assert!(idle.imp().others.get_visible());
    let idle_released = Rc::new(RefCell::new(Vec::<u64>::new()));
    idle.connect_release_requested({
        let idle_released = Rc::clone(&idle_released);
        move |_, id| idle_released.borrow_mut().push(id)
    });
    let holder = children_of::<Expandable>(&*idle.imp().list).remove(0);
    holder.head::<LegacyRow>().expect("a row").emit_clicked();
    assert!(holder.expanded() && dimmed(idle.imp().hero.upcast_ref()));
    holder
        .details::<gtk4::Box>()
        .and_then(|card| card.last_child())
        .and_downcast::<LegacyRow>()
        .expect("Release")
        .emit_clicked();
    assert_eq!(*idle_released.borrow(), [1]);

    idle.set_inhibitors(&[]);
    assert!(
        !idle.imp().others.get_visible() && !dimmed(idle.imp().hero.upcast_ref()),
        "the section and the dimming clear when the list empties"
    );

    let idle_footer_activated = Rc::new(Cell::new(0u32));
    idle.connect_footer_activated({
        let idle_footer_activated = Rc::clone(&idle_footer_activated);
        move |_| idle_footer_activated.set(idle_footer_activated.get() + 1)
    });
    idle.set_footer(Some("Idle settings"));
    assert!(
        !idle.imp().shell.imp().footer_rule.get_visible(),
        "an empty inhibitor list leaves no divider between Keep awake and settings"
    );
    idle.set_inhibitors(&[inhibitor(2, "Firefox", false)]);
    assert!(idle.imp().shell.imp().footer_rule.get_visible());
    idle.set_inhibitors(&[]);
    assert!(!idle.imp().shell.imp().footer_rule.get_visible());
    idle.imp().footer.emit_clicked();
    assert_eq!(idle_footer_activated.get(), 1);
}
