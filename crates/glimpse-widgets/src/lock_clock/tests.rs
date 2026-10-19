use super::*;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn ordinal_follows_english_rules() {
    let cases = [
        (1, "1st"),
        (2, "2nd"),
        (3, "3rd"),
        (4, "4th"),
        (11, "11th"),
        (12, "12th"),
        (13, "13th"),
        (21, "21st"),
        (22, "22nd"),
        (23, "23rd"),
        (31, "31st"),
    ];
    for (day, expected) in cases {
        assert_eq!(day_label(day, "en_US.UTF-8"), expected, "day {day}");
    }
    assert_eq!(day_label(1, "C"), "1st");
    assert_eq!(day_label(2, "C.UTF-8"), "2nd");
    assert_eq!(day_label(3, "C.utf8"), "3rd");
    assert_eq!(day_label(3, "POSIX"), "3rd");
}

#[test]
fn a_bad_format_warns_once_per_pattern() {
    assert!(warn_once("%Q-test-a"));
    assert!(!warn_once("%Q-test-a"), "the same pattern again is quiet");
    assert!(warn_once("%Q-test-b"), "a different pattern warns");
}

#[test]
fn a_non_english_date_shows_the_plain_day() {
    assert_eq!(
        day_label(23, "pl_PL.UTF-8"),
        "23",
        "a Polish date is 'środa, 23 września', never '23rd'"
    );
    assert_eq!(day_label(1, "de_DE.UTF-8"), "1");
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

    clock.set_formats("%H:%M", "%A, {day} %B");
    clock.set_time(&now);
    assert_eq!(imp.time.text().as_str(), "14:07");
    assert_eq!(imp.date.text().as_str(), "Sunday, 1st November");
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

    clock.set_formats("%H:%M", "%A, {day} %B");
    clock.set_time(&now);
    assert!(imp.date.get_visible());
    assert_eq!(imp.date.text().as_str(), "Sunday, 1st November");

    window.destroy();
}
