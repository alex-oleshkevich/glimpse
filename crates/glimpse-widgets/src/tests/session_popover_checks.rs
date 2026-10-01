use super::*;

pub(super) fn check() {
    let session = SessionPopover::new();
    assert!(
        session
            .layout_manager()
            .is_some_and(|layout| layout.is::<gtk4::BinLayout>()),
        "the session popover root has a bin layout"
    );
    let session_actions = Rc::new(RefCell::new(Vec::new()));
    session.connect_action_requested({
        let session_actions = Rc::clone(&session_actions);
        move |_, action| session_actions.borrow_mut().push(action.to_owned())
    });
    let hibernate = SessionActionState {
        visible: true,
        enabled: false,
        subtitle: Some("Blocked by an active inhibitor.".to_owned()),
    };
    session.set_action(HIBERNATE, &hibernate);
    assert!(!session.imp().hibernate.is_sensitive());
    let notifies = Rc::new(Cell::new(0u32));
    session.imp().hibernate.connect_notify_local(None, {
        let notifies = Rc::clone(&notifies);
        move |_, _| notifies.set(notifies.get() + 1)
    });
    session.set_action(HIBERNATE, &hibernate);
    assert_eq!(
        notifies.get(),
        0,
        "an unchanged action must not write the row"
    );
    session.imp().lock.emit_clicked();
    assert_eq!(*session_actions.borrow(), [LOCK]);
}
