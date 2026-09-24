mod imp;

use gtk4::{glib, subclass::prelude::*};

#[cfg(test)]
use gtk4::prelude::*;

use std::cell::RefCell;
use std::collections::HashSet;

use crate::set_text;

glib::wrapper! {
    pub struct LockClock(ObjectSubclass<imp::LockClock>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for LockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl LockClock {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_formats(&self, time: &str, date: &str) {
        let imp = self.imp();
        imp.time_format.replace(time.to_owned());
        imp.date_format.replace(date.to_owned());
    }

    pub fn set_time(&self, time: &glib::DateTime) {
        let imp = self.imp();

        let time_text = formatted(time, imp.time_format.borrow().as_str());
        set_text(&imp.time, Some(&time_text));

        let day = day_label(time.day_of_month() as u32, &time_locale());
        let date_format = imp.date_format.borrow().replace("{day}", &day);
        let date_text = formatted(time, &date_format);
        set_text(&imp.date, Some(&date_text));
    }
}

thread_local! {
    static WARNED: RefCell<HashSet<String>> = RefCell::default();
}

fn formatted(time: &glib::DateTime, pattern: &str) -> String {
    match time.format(pattern) {
        Ok(text) => text.to_string(),
        Err(_) => {
            if warn_once(pattern) {
                tracing::warn!(pattern, "the lock clock format does not format; hiding it");
            }
            String::new()
        }
    }
}

fn warn_once(pattern: &str) -> bool {
    WARNED.with_borrow_mut(|warned| warned.insert(pattern.to_owned()))
}

fn time_locale() -> String {
    let name = unsafe { libc::setlocale(libc::LC_TIME, std::ptr::null()) };
    if name.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned()
}

fn day_label(day: u32, time_locale: &str) -> String {
    let english = matches!(time_locale, "" | "C" | "POSIX")
        || time_locale.starts_with("C.")
        || time_locale.starts_with("en");
    if !english {
        return day.to_string();
    }
    let suffix = match (day % 100, day % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{day}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
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
        if gtk4::init().is_err() {
            return;
        }
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
}
