mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct MediaScrubber(ObjectSubclass<imp::MediaScrubber>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl MediaScrubber {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_progress(&self, position_seconds: f64, length_seconds: f64) {
        let imp = self.imp();
        let upper = length_seconds.max(0.0).max(f64::EPSILON);
        // Guard the whole transition: GTK can emit `value-changed` from
        // `set_upper` alone if the current value is clamped down.
        imp.updating.set(true);
        imp.scale.adjustment().set_upper(upper);
        let value = position_seconds.clamp(0.0, upper);
        imp.scale.set_value(value);
        imp.updating.set(false);
    }

    pub fn set_seekable(&self, seekable: bool) {
        self.imp().seekable.set(seekable);
        self.imp().scale.set_sensitive(seekable);
    }

    pub fn connect_seek_requested(
        &self,
        f: impl Fn(&Self, f64) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "seek-requested",
            false,
            closure_local!(move |scrubber: &Self, seconds: f64| f(scrubber, seconds)),
        )
    }
}

impl Default for MediaScrubber {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn active_drag_value_changes_emit_seek_requests() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let scrubber = MediaScrubber::new();
        scrubber.set_seekable(true);
        scrubber.set_progress(10.0, 120.0);

        let emitted = Rc::new(Cell::new(None));
        scrubber.connect_seek_requested({
            let emitted = emitted.clone();
            move |_, seconds| emitted.set(Some(seconds))
        });

        scrubber.imp().interacting.set(true);
        scrubber.imp().scale.set_value(42.0);

        assert_eq!(emitted.get(), Some(42.0));
    }

    #[test]
    fn programmatic_progress_updates_do_not_emit_seek_requests() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let scrubber = MediaScrubber::new();
        scrubber.set_seekable(true);

        let emissions = Rc::new(Cell::new(0));
        scrubber.connect_seek_requested({
            let emissions = emissions.clone();
            move |_, _| emissions.set(emissions.get() + 1)
        });

        scrubber.set_progress(10.0, 120.0);
        scrubber.set_progress(20.0, 120.0);

        assert_eq!(emissions.get(), 0);
    }

    #[test]
    fn progress_updates_reset_value_even_while_interacting() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let scrubber = MediaScrubber::new();
        scrubber.set_seekable(true);
        scrubber.set_progress(40.0, 120.0);

        scrubber.imp().interacting.set(true);
        scrubber.set_progress(0.0, 180.0);

        assert_eq!(scrubber.imp().scale.value(), 0.0);
    }
}
