use super::*;

#[test]
#[ignore = "needs a display"]
fn workspace_name_popover_widgets() {
    if gtk4::init().is_err() {
        return;
    }
    register_resources().expect("resources");

    let popover = WorkspaceNamePopover::new();
    let imp = popover.imp();

    popover.set_workspace("Workspace 3");
    assert_eq!(imp.shell.title().as_deref(), Some("Workspace 3"));
    assert_eq!(imp.shell.subtitle(), None);

    popover.set_name("dev");
    assert_eq!(imp.name.text(), "dev", "an untouched entry takes the name");

    let writes = Rc::new(Cell::new(0));
    let counter = writes.clone();
    imp.name
        .connect_notify_local(Some("text"), move |_, _| counter.set(counter.get() + 1));
    popover.set_name("dev");
    assert_eq!(writes.get(), 0, "an unchanged name writes nothing");

    imp.name.set_text("chat");
    popover.set_name("mail");
    assert_eq!(
        imp.name.text(),
        "chat",
        "a reconcile must not clobber what the user is typing"
    );

    let submitted = Rc::new(RefCell::new(None));
    let sink = submitted.clone();
    popover.connect_submitted(move |_, name| {
        sink.replace(Some(name));
    });
    imp.name.emit_activate();
    assert_eq!(submitted.borrow().as_deref(), Some("chat"));

    popover.set_taken(&[(
        "Chat".to_owned(),
        "Already the name of Workspace 2".to_owned(),
    )]);
    assert!(
        imp.taken.get_visible(),
        "a name another workspace holds warns at once"
    );
    submitted.replace(None);
    imp.name.emit_activate();
    assert_eq!(
        submitted.borrow().as_deref(),
        None,
        "and Enter does nothing"
    );
    imp.name.set_text("code");
    assert!(!imp.taken.get_visible());

    popover.set_recent(&["music".to_owned()]);
    assert!(imp.recent.get_visible());
    imp.recent_rows
        .first_child()
        .and_downcast::<ActionRow>()
        .expect("a recent name")
        .activate();
    assert_eq!(submitted.borrow().as_deref(), Some("music"));

    imp.name
        .emit_by_name::<()>("icon-press", &[&gtk4::EntryIconPosition::Secondary]);
    assert_eq!(imp.name.text(), "", "the clear icon empties the entry");

    let cancelled = Rc::new(Cell::new(false));
    let sink = cancelled.clone();
    popover.connect_cancelled(move |_| sink.set(true));
    popover.emit_by_name::<()>("cancelled", &[]);
    assert!(cancelled.get());

}
