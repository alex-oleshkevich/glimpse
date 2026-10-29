use super::*;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn a_bad_format_warns_once_per_pattern() {
    assert!(warn_once("%Q-test-a"));
    assert!(!warn_once("%Q-test-a"), "the same pattern again is quiet");
    assert!(warn_once("%Q-test-b"), "a different pattern warns");
}

#[test]
#[ignore = "needs a display"]
fn lock_clock_states() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");
    unsafe {
        gettextrs::setlocale(gettextrs::LocaleCategory::LcTime, "C");
    }

    let clock = LockClock::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&clock));
    let imp = clock.imp();

    assert!(
        !imp.time.get_visible() && !imp.date.get_visible(),
        "both labels start hidden, matching the blueprint"
    );

    let notifications = Rc::new(Cell::new(0));
    imp.time.connect_notify_local(Some("label"), {
        let notifications = Rc::clone(&notifications);
        move |_, _| notifications.set(notifications.get() + 1)
    });

    let now = glib::DateTime::from_utc(2026, 11, 1, 14, 7, 0.0).expect("instant");
    clock.set_formats("%H:%M", "");
    clock.set_time(&now);
    assert!(
        !imp.date.get_visible(),
        "an empty date format on the very first set_time leaves the date hidden"
    );
    assert_eq!(imp.time.text().as_str(), "14:07");

    clock.set_formats("%H:%M", "%A, %-d %B");
    clock.set_time(&now);
    assert_eq!(imp.time.text().as_str(), "14:07");
    assert_eq!(imp.date.text().as_str(), "Sunday, 1 November");
    assert_eq!(notifications.get(), 1);

    let same_minute = glib::DateTime::from_utc(2026, 11, 1, 14, 7, 45.0).expect("instant");
    clock.set_time(&same_minute);
    assert_eq!(notifications.get(), 1, "an unchanged minute writes nothing");

    clock.set_formats("%H:%M", "");
    clock.set_time(&now);
    assert!(
        !imp.date.get_visible(),
        "an empty date format hides the label"
    );

    clock.set_formats("%H:%M", "%A, %-d %B");
    clock.set_time(&now);
    assert!(imp.date.get_visible());
    assert_eq!(imp.date.text().as_str(), "Sunday, 1 November");

    window.destroy();
}
