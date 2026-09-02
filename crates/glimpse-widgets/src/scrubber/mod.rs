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

pub(crate) fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0).round() as u64;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match hours {
        0 => format!("{minutes}:{seconds:02}"),
        hours => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::clock;

    #[test]
    fn a_clock_pads_seconds_but_not_the_leading_unit() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(9.0), "0:09");
        assert_eq!(clock(167.0), "2:47");
        assert_eq!(clock(600.0), "10:00");
    }

    #[test]
    fn an_hour_adds_a_unit_and_pads_the_minutes_behind_it() {
        assert_eq!(clock(3600.0), "1:00:00");
        assert_eq!(clock(3661.0), "1:01:01");
        assert_eq!(clock(45296.0), "12:34:56");
    }

    #[test]
    fn a_negative_position_reads_as_the_start_rather_than_wrapping() {
        assert_eq!(clock(-1.0), "0:00");
        assert_eq!(clock(f64::NEG_INFINITY), "0:00");
    }

    #[test]
    fn a_fraction_rounds_rather_than_truncating() {
        assert_eq!(clock(59.6), "1:00");
        assert_eq!(clock(0.4), "0:00");
    }
}
