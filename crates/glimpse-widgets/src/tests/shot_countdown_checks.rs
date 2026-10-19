use super::*;

#[test]
#[ignore = "needs a display"]
fn shot_countdown_widgets() {
    shot_display();

    let countdown = ShotCountdown::new();
    let imp = countdown.imp();
    assert_eq!(imp.label.text(), "Capturing in 0");
    assert!(!imp.label.uses_markup());
    let writes = Rc::new(Cell::new(0));
    imp.label.connect_notify_local(Some("label"), {
        let writes = Rc::clone(&writes);
        move |_, _| writes.set(writes.get() + 1)
    });
    countdown.set_seconds(3);
    assert_eq!(imp.label.text(), "Capturing in 3");
    let written = writes.get();
    countdown.set_seconds(3);
    assert_eq!(writes.get(), written);
    let cancelled = Rc::new(Cell::new(0));
    countdown.connect_cancelled({
        let cancelled = Rc::clone(&cancelled);
        move |_| cancelled.set(cancelled.get() + 1)
    });
    imp.cancel.emit_clicked();
    assert_eq!(cancelled.get(), 1);
}
