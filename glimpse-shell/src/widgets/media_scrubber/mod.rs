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
        let value = position_seconds.clamp(0.0, upper);
        // Guard the whole transition: GTK can emit `value-changed` from
        // `set_upper` alone if the current value is clamped down.
        imp.updating.set(true);
        imp.scale.adjustment().set_upper(upper);
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
