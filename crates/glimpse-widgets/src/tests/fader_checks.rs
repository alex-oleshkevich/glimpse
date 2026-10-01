use super::*;

pub(super) fn check() {
    let fader = Fader::new();
    let mute = child_named::<gtk4::ToggleButton>(&fader, "fader__mute");
    let track = child_named::<gtk4::Scale>(&fader, "fader__track");
    fader.set_value(62.0);
    assert_eq!(fader.value(), 62.0);

    fader.set_value(150.0);
    assert_eq!(
        fader.value(),
        100.0,
        "a device already above 100% is pinned at the top of the fader's own range rather \
             than pretending the extra headroom exists"
    );

    let redraws = Rc::new(Cell::new(0u32));
    track.connect_value_changed({
        let redraws = Rc::clone(&redraws);
        move |_| redraws.set(redraws.get() + 1)
    });
    fader.set_value(100.0);
    assert_eq!(redraws.get(), 0, "an unchanged value is not reapplied");

    assert_eq!(
        (
            track.adjustment().step_increment(),
            track.adjustment().page_increment()
        ),
        (1.0, 5.0),
        "the increments are set from Rust, as a function of the configured maximum, \
             because blueprint-compiler's adjustment rule rejects an adjustment carrying \
             anything besides lower, upper and value"
    );

    let changed = Rc::new(RefCell::new(Vec::new()));
    fader.connect_changed({
        let changed = Rc::clone(&changed);
        move |_, value| changed.borrow_mut().push(value)
    });
    fader.set_value(38.0);
    assert!(
        changed.borrow().is_empty(),
        "a programmatic set_value renders the new position without reporting it back, or a \
             state update arriving from the backend would look exactly like a drag"
    );

    fader.imp().held.set(Some(38.0));
    fader.set_value(80.0);
    assert_eq!(
        fader.value(),
        38.0,
        "a value arriving mid-drag loses to the drag in progress"
    );
    fader.imp().held.set(None);
    fader.set_value(80.0);
    assert_eq!(fader.value(), 80.0);

    let toggles = Rc::new(RefCell::new(Vec::new()));
    fader.connect_toggled({
        let toggles = Rc::clone(&toggles);
        move |_, muted| toggles.borrow_mut().push(muted)
    });

    fader.set_muted(true);
    assert!(mute.is_active() && fader.muted());
    assert!(
        toggles.borrow().is_empty(),
        "showing a muted state the caller already knows about must not report it back"
    );

    fader.set_muted(true);
    assert!(
        toggles.borrow().is_empty(),
        "an unchanged muted flag is not reapplied"
    );

    mute.set_active(false);
    assert_eq!(
        *toggles.borrow(),
        [false],
        "a click on the mute button is the one thing that reports back"
    );

    assert_eq!(fader.icon_name(), None);
    let icon_changes = Rc::new(Cell::new(0u32));
    mute.connect_icon_name_notify({
        let icon_changes = Rc::clone(&icon_changes);
        move |_| icon_changes.set(icon_changes.get() + 1)
    });
    fader.set_icon_name(Some("audio-volume-high-symbolic".to_owned()));
    assert_eq!(
        fader.icon_name().as_deref(),
        Some("audio-volume-high-symbolic")
    );
    assert_eq!(icon_changes.get(), 1);
    fader.set_icon_name(Some("audio-volume-high-symbolic".to_owned()));
    assert_eq!(icon_changes.get(), 1, "an equal icon name is not reapplied");

    let fader = Fader::new();
    assert_eq!(fader.maximum(), 100.0);

    let track = child_named::<gtk4::Scale>(&fader, "fader__track");
    let page_notifies = Rc::new(Cell::new(0u32));
    track
        .adjustment()
        .connect_notify_local(Some("page-increment"), {
            let page_notifies = Rc::clone(&page_notifies);
            move |_, _| page_notifies.set(page_notifies.get() + 1)
        });

    fader.set_maximum(3.0);
    assert_eq!(
        (
            track.adjustment().step_increment(),
            track.adjustment().page_increment()
        ),
        (1.0, 1.0),
        "max(1, max/100) and max(1, max/20) both floor at 1 on a three-position control"
    );
    assert_eq!(page_notifies.get(), 1);
    fader.set_value(4.0);
    assert_eq!(
        fader.value(),
        3.0,
        "set_value clamps to the configured maximum, never a literal 100"
    );
    fader.set_maximum(3.0);
    assert_eq!(
        page_notifies.get(),
        1,
        "an unchanged maximum is not reapplied"
    );

    let fader = Fader::new();
    fader.set_value(60.0);
    fader.set_maximum(3.0);
    assert_eq!(
        fader.value(),
        3.0,
        "lowering the maximum below the current value must not leave the slider drawn \
             past the end of its own track"
    );

    let fader = Fader::new();
    fader.set_maximum(-5.0);
    assert_eq!(
        fader.maximum(),
        0.0,
        "a negative maximum is clamped inside the setter itself — a paramspec minimum \
             bound was tried first and measured to panic on this exact input, since \
             glib-rs treats the C side's own silent coercion as an error"
    );
    fader.set_value(1.0);
    assert_eq!(
        fader.value(),
        0.0,
        "a maximum of zero leaves nothing to set a positive value to"
    );

    let fader = Fader::new();
    fader.set_maximum(400000.0);
    let track = child_named::<gtk4::Scale>(&fader, "fader__track");
    assert_eq!(
        (
            track.adjustment().step_increment(),
            track.adjustment().page_increment()
        ),
        (4000.0, 20000.0),
        "increments scale with a wide native range"
    );
    let changed = Rc::new(RefCell::new(Vec::new()));
    fader.connect_changed({
        let changed = Rc::clone(&changed);
        move |_, value| changed.borrow_mut().push(value)
    });
    let _: bool = track.emit_by_name("change-value", &[&gtk4::ScrollType::None, &400000.0f64]);
    assert_eq!(
        *changed.borrow(),
        [400000.0],
        "GtkRange's own class handler re-clamps the adjustment to its current bounds \
             after every connected handler runs, so value() settles at 400000.0 whether or \
             not this handler's own clamp used the configured maximum — only the changed \
             signal, read from inside the handler before that class handler runs, tells a \
             fixed clamp apart from a literal 100.0"
    );

    let fader = Fader::new();
    assert_eq!(fader.floor(), 0.0);
    assert_eq!(fader.maximum(), 100.0);

    fader.set_floor(5.0);
    assert_eq!(fader.floor(), 5.0);
    fader.set_value(0.0);
    assert_eq!(
        fader.value(),
        5.0,
        "set_value clamps to the configured floor, never a literal 0"
    );

    fader.set_value(50.0);
    let track = child_named::<gtk4::Scale>(&fader, "fader__track");
    let changed = Rc::new(RefCell::new(Vec::new()));
    fader.connect_changed({
        let changed = Rc::clone(&changed);
        move |_, value| changed.borrow_mut().push(value)
    });
    let _: bool = track.emit_by_name("change-value", &[&gtk4::ScrollType::None, &0.0f64]);
    assert_eq!(
        *changed.borrow(),
        [5.0],
        "GtkRange's own class handler re-clamps the adjustment to its current bounds \
             after every connected handler runs, so value() settles at the floor whether or \
             not this handler's own clamp used the configured floor — only the changed \
             signal, read from inside the handler before that class handler runs, tells a \
             floor-aware clamp apart from a literal 0.0"
    );

    let fader = Fader::new();
    fader.set_value(10.0);
    fader.set_floor(50.0);
    assert_eq!(
        fader.value(),
        50.0,
        "GtkAdjustment::set_lower does not re-clamp value on its own, so a raised floor \
             pulls the value up to it explicitly"
    );

    let fader = Fader::new();
    fader.set_floor(150.0);
    assert_eq!(
        fader.floor(),
        100.0,
        "a floor above the current maximum is clamped down to it rather than inverting \
             the range"
    );
    assert_eq!(
        fader.maximum(),
        100.0,
        "the maximum itself is left untouched"
    );

    let fader = Fader::new();
    fader.set_floor(-5.0);
    assert_eq!(
        fader.floor(),
        0.0,
        "a negative floor is clamped inside the setter itself, exactly as a negative \
             maximum already is"
    );

    let fader = Fader::new();
    assert!(fader.toggleable());
    let mute = child_named::<gtk4::ToggleButton>(&fader, "fader__mute");
    let icon = child_named::<gtk4::Image>(&fader, "fader__icon");
    assert!(mute.get_visible());
    assert!(!icon.get_visible());

    fader.set_toggleable(false);
    assert!(!fader.toggleable());
    assert!(!mute.get_visible());
    assert!(icon.get_visible());
    assert!(
        fader.is_sensitive(),
        "a plain icon presentation has nothing to mute and must not disable the fader"
    );
    assert!(
        icon.is_sensitive(),
        "an insensitive image renders dimmed, which is the exact bug being fixed"
    );

    fader.set_icon_name(Some("display-brightness-symbolic".to_owned()));
    assert_eq!(
        icon.icon_name().as_deref(),
        Some("display-brightness-symbolic"),
        "icon-name sets the icon under either presentation"
    );

    let visible_notifies = Rc::new(Cell::new(0u32));
    icon.connect_notify_local(Some("visible"), {
        let visible_notifies = Rc::clone(&visible_notifies);
        move |_, _| visible_notifies.set(visible_notifies.get() + 1)
    });
    fader.set_toggleable(false);
    assert_eq!(
        visible_notifies.get(),
        0,
        "an unchanged toggleable is not reapplied"
    );

    fader.set_toggleable(true);
    assert!(mute.get_visible());
    assert!(!icon.get_visible());
}
