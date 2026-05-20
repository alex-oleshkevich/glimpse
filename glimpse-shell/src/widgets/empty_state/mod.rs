mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct EmptyState(ObjectSubclass<imp::EmptyState>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl EmptyState {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_title(&self, text: &str) {
        self.imp().title.set_text(text);
    }

    pub fn set_subtitle(&self, text: Option<&str>) {
        let subtitle = &self.imp().subtitle;
        match text {
            Some(text) if !text.is_empty() => {
                subtitle.set_text(text);
                subtitle.set_visible(true);
            }
            _ => {
                subtitle.set_text("");
                subtitle.set_visible(false);
            }
        }
    }
}

impl Default for EmptyState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn empty_state_centers_content_and_expands_horizontally() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let empty = EmptyState::new();

        assert!(empty.has_css_class("empty-state"));
        assert!(empty.hexpands());
        assert!(empty.vexpands());
        assert_eq!(empty.halign(), gtk4::Align::Fill);
        assert_eq!(empty.valign(), gtk4::Align::Center);
        assert!(empty.imp().title.has_css_class("empty-state__title"));
        assert!(empty.imp().subtitle.has_css_class("empty-state__subtitle"));
        assert_eq!(empty.imp().title.xalign(), 0.5);
        assert_eq!(empty.imp().subtitle.xalign(), 0.5);
    }

    #[test]
    fn empty_state_subtitle_hides_when_missing() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let empty = EmptyState::new();

        empty.set_subtitle(Some("line 2"));
        assert!(empty.imp().subtitle.is_visible());

        empty.set_subtitle(None);
        assert!(!empty.imp().subtitle.is_visible());
    }
}
