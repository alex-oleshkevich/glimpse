mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct MediaMeta(ObjectSubclass<imp::MediaMeta>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl MediaMeta {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_title(&self, text: &str) {
        self.imp().title.set_text(text);
    }

    pub fn set_subtitle(&self, text: &str) {
        let label = &self.imp().subtitle;
        label.set_text(text);
        label.set_visible(!text.is_empty());
    }

    /// When true, the title is forced to a single line with ellipsize-end —
    /// matches secondary-row use. When false (default), the title may wrap
    /// to two lines (used by the main now-playing card).
    pub fn set_single_line(&self, single_line: bool) {
        let title = &self.imp().title;
        if single_line {
            title.set_single_line_mode(true);
            title.set_wrap(false);
            title.set_lines(1);
        } else {
            title.set_single_line_mode(false);
            title.set_wrap(true);
            title.set_lines(2);
        }
    }

    pub fn connect_activated(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            closure_local!(move |meta: &Self| f(meta)),
        )
    }
}

impl Default for MediaMeta {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;
    use gtk4::pango::{EllipsizeMode, WrapMode};

    #[test]
    fn title_wraps_to_two_lines_after_threshold() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let meta = MediaMeta::new();
        let title = &meta.imp().title;

        assert!(title.wraps());
        assert_eq!(title.wrap_mode(), WrapMode::WordChar);
        assert_eq!(title.lines(), 2);
        assert_eq!(title.max_width_chars(), 28);
        assert!(!title.is_single_line_mode());
        assert_eq!(title.ellipsize(), EllipsizeMode::End);
    }

    #[test]
    fn mpris_meta_label_colors_beat_theme_label_inheritance() {
        let css = include_str!("../../../../themes/base.css");

        assert!(css.contains("label.mpris-meta__title"));
        assert!(css.contains("label.mpris-meta__subtitle"));
        assert!(!css.contains("color: red;"));
    }
}
