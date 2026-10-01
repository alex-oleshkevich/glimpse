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
mod tests;
