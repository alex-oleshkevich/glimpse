mod imp;

use gtk4::{glib, prelude::*};

glib::wrapper! {
    pub struct MutedIndicator(ObjectSubclass<imp::MutedIndicator>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl MutedIndicator {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_active(&self, active: bool) {
        self.set_visible(active);
        if active {
            self.add_css_class("is-active");
        } else {
            self.remove_css_class("is-active");
        }
    }
}

impl Default for MutedIndicator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn muted_indicator_has_css_class() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let indicator = MutedIndicator::new();

        assert!(indicator.has_css_class("muted-indicator"));
        assert!(!indicator.is_visible());
    }

    #[test]
    fn set_active_toggles_visibility_and_active_class() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let indicator = MutedIndicator::new();

        indicator.set_active(true);
        assert!(indicator.is_visible());
        assert!(indicator.has_css_class("is-active"));

        indicator.set_active(false);
        assert!(!indicator.is_visible());
        assert!(!indicator.has_css_class("is-active"));
    }
}
