mod imp;

use glib::closure_local;
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::widgets::media_transport::PlayState;

glib::wrapper! {
    pub struct SecondaryPlayerRow(ObjectSubclass<imp::SecondaryPlayerRow>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl SecondaryPlayerRow {
    pub fn new() -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.imp().artwork.set_artwork_size(48);
        obj.imp().artwork.set_fallback_icon_pixel_size(22);
        obj.imp().meta.set_single_line(true);
        obj
    }

    pub fn set_title(&self, text: &str) {
        self.imp().meta.set_title(text);
    }

    pub fn set_subtitle(&self, text: &str) {
        self.imp().meta.set_subtitle(text);
    }

    pub fn set_artwork(&self, paintable: Option<&gdk::Texture>) {
        self.imp().artwork.set_paintable(paintable);
    }

    pub fn set_play_state(&self, state: PlayState) {
        let icon = match state {
            PlayState::Playing => "media-playback-pause-symbolic",
            PlayState::Paused => "media-playback-start-symbolic",
        };
        self.imp().play_pause.set_icon_name(icon);
    }

    pub fn set_can_play_pause(&self, can: bool) {
        self.imp().play_pause.set_sensitive(can);
    }

    pub fn set_can_next(&self, can: bool) {
        self.imp().next.set_sensitive(can);
    }

    pub fn connect_play_pause(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "play-pause-clicked",
            false,
            closure_local!(move |row: &Self| f(row)),
        )
    }

    pub fn connect_next(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "next-clicked",
            false,
            closure_local!(move |row: &Self| f(row)),
        )
    }

    pub fn connect_activated(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("activated", false, closure_local!(move |row: &Self| f(row)))
    }
}

impl Default for SecondaryPlayerRow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;

    #[test]
    fn template_wires_row_sections_and_controls() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let row = SecondaryPlayerRow::new();

        assert_eq!(row.orientation(), gtk4::Orientation::Horizontal);
        assert_eq!(row.spacing(), 2);
        assert!(row.has_css_class("mpris-row"));
        assert!(row.imp().artwork.has_css_class("mpris-row__artwork"));
        assert!(row.imp().meta.has_css_class("mpris-row__meta"));
        assert!(row.imp().play_pause.has_css_class("mpris-row__button"));
        assert!(row.imp().play_pause.has_css_class("mpris-row__play"));
        assert!(row.imp().next.has_css_class("mpris-row__button"));
        assert!(row.imp().next.has_css_class("mpris-row__next"));
    }
}
