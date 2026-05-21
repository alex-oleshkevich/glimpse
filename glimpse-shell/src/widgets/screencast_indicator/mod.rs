mod imp;

use std::time::Duration;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct ScreenCastIndicator(ObjectSubclass<imp::ScreenCastIndicator>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl ScreenCastIndicator {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Show the indicator when `active` is true, hide otherwise.
    pub fn set_active(&self, active: bool) {
        self.set_visible(active);
    }

    /// Update the timer text. Formats as `MM:SS` until 60 minutes,
    /// then promotes to `H:MM:SS`.
    pub fn set_elapsed(&self, duration: Duration) {
        let secs = duration.as_secs();
        let label = if secs >= 3600 {
            format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
        } else {
            format!("{:02}:{:02}", secs / 60, secs % 60)
        };
        self.set_timer_text(&label);
    }

    /// Set the timer text directly — for callers that already format the
    /// elapsed duration themselves (e.g. matching an existing convention).
    pub fn set_timer_text(&self, text: &str) {
        self.imp().timer_label.set_text(text);
    }
}

impl Default for ScreenCastIndicator {
    fn default() -> Self {
        Self::new()
    }
}
