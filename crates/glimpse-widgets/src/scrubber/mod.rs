mod imp;

use gtk4::{glib, prelude::*};

glib::wrapper! {
    pub struct Scrubber(ObjectSubclass<imp::Scrubber>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Scrubber {
    fn default() -> Self {
        Self::new()
    }
}

impl Scrubber {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_seek<F: Fn(&Self, f64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "seek",
            false,
            glib::closure_local!(move |scrubber: Self, seconds: f64| f(&scrubber, seconds)),
        )
    }
}

/// `m:ss`, gaining an `h:` only when it needs one. Public because the bar renders the same track
/// beside this widget, and two spellings of one elapsed time disagree by a second.
pub fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0).round() as u64;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match hours {
        0 => format!("{minutes}:{seconds:02}"),
        hours => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

#[cfg(test)]
mod tests;
