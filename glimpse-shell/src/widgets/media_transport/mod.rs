mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
}

glib::wrapper! {
    pub struct MediaTransport(ObjectSubclass<imp::MediaTransport>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl MediaTransport {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_play_state(&self, state: PlayState) {
        let icon = match state {
            PlayState::Playing => "media-playback-pause-symbolic",
            PlayState::Paused => "media-playback-start-symbolic",
        };
        self.imp().play_pause.set_icon_name(icon);
    }

    pub fn set_can_previous(&self, can: bool) {
        self.imp().previous.set_sensitive(can);
    }

    pub fn set_can_play_pause(&self, can: bool) {
        self.imp().play_pause.set_sensitive(can);
    }

    pub fn set_can_next(&self, can: bool) {
        self.imp().next.set_sensitive(can);
    }

    pub fn connect_previous(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "previous-clicked",
            false,
            closure_local!(move |this: &Self| f(this)),
        )
    }

    pub fn connect_play_pause(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "play-pause-clicked",
            false,
            closure_local!(move |this: &Self| f(this)),
        )
    }

    pub fn connect_next(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "next-clicked",
            false,
            closure_local!(move |this: &Self| f(this)),
        )
    }
}

impl Default for MediaTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;

    #[test]
    fn play_button_uses_explicit_larger_icon() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let transport = MediaTransport::new();

        assert_eq!(transport.imp().play_icon.pixel_size(), 22);
    }

    #[test]
    fn play_state_updates_explicit_play_icon() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let transport = MediaTransport::new();

        transport.set_play_state(PlayState::Playing);
        assert_eq!(
            transport.imp().play_icon.icon_name().as_deref(),
            Some("media-playback-pause-symbolic")
        );

        transport.set_play_state(PlayState::Paused);
        assert_eq!(
            transport.imp().play_icon.icon_name().as_deref(),
            Some("media-playback-start-symbolic")
        );
    }
}
