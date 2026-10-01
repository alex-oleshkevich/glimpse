use super::*;

pub(super) fn check() {
    let toggle = SwitchRow::new();
    let flips = Rc::new(RefCell::new(Vec::new()));
    toggle.connect_toggled({
        let flips = Rc::clone(&flips);
        move |_, on| flips.borrow_mut().push(on)
    });

    toggle
        .upcast_ref::<Row>()
        .set_title(Some("Connect automatically"));
    let knob = child_named::<gtk4::Switch>(&toggle, "switch-row__knob");
    toggle.set_active(true);
    assert!(toggle.active() && knob.is_active());
    assert!(
        flips.borrow().is_empty(),
        "showing the state the caller already knows about must not report it back"
    );

    toggle.emit_clicked();
    assert_eq!(
        *flips.borrow(),
        [false],
        "the row body is the target too, and it reports exactly one flip"
    );
    assert!(!toggle.active(), "the knob follows the row it belongs to");

    knob.set_active(true);
    assert_eq!(
        *flips.borrow(),
        [false, true],
        "the knob and the row body share one emitter, so neither doubles the other"
    );

    assert!(!toggle.locked());
    toggle.set_locked(true);
    assert!(!knob.is_sensitive(), "a locked row disables the knob");
    assert!(
        toggle.is_sensitive(),
        "the row itself stays sensitive so its subtitle is not dimmed along with the knob"
    );

    let sensitive_changes = Rc::new(Cell::new(0u32));
    knob.connect_sensitive_notify({
        let sensitive_changes = Rc::clone(&sensitive_changes);
        move |_| sensitive_changes.set(sensitive_changes.get() + 1)
    });
    toggle.set_locked(true);
    assert_eq!(
        sensitive_changes.get(),
        0,
        "locking an already-locked row does not touch the knob a second time"
    );

    let flips_before = flips.borrow().len();
    toggle.emit_clicked();
    assert_eq!(
        flips.borrow().len(),
        flips_before,
        "a locked row's body click is inert"
    );
    assert!(
        toggle.active(),
        "a locked row does not change value on a body click"
    );

    toggle.set_active(false);
    assert!(
        !toggle.active() && !knob.is_active(),
        "render still reaches a locked switch"
    );
    assert_eq!(
        flips.borrow().len(),
        flips_before,
        "a programmatic set stays quiet under lock, same as unlocked"
    );

    toggle.set_locked(false);
    assert_eq!(sensitive_changes.get(), 1);
    assert!(knob.is_sensitive());
    toggle.emit_clicked();
    assert_eq!(
        flips.borrow().len(),
        flips_before + 1,
        "unlocking gives the row body its click back"
    );
}
