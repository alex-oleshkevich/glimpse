mod imp;

use gtk4::{glib, subclass::prelude::*};

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

        let date_text = formatted(time, imp.date_format.borrow().as_str());
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

#[cfg(test)]
mod tests;
