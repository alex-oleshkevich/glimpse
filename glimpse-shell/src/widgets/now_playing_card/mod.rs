mod imp;

use gtk4::{gdk, glib, subclass::prelude::*};

use crate::widgets::{
    media_artwork::MediaArtwork,
    media_meta::MediaMeta,
    media_scrubber::MediaScrubber,
    media_transport::{MediaTransport, PlayState},
    scrubber_times::ScrubberTimes,
};

glib::wrapper! {
    pub struct NowPlayingCard(ObjectSubclass<imp::NowPlayingCard>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl NowPlayingCard {
    pub fn new() -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.imp().artwork.set_artwork_size(72);
        obj.imp().artwork.set_fallback_icon_pixel_size(18);
        obj
    }

    pub fn artwork(&self) -> &MediaArtwork {
        &self.imp().artwork
    }

    pub fn meta(&self) -> &MediaMeta {
        &self.imp().meta
    }

    pub fn scrubber(&self) -> &MediaScrubber {
        &self.imp().scrubber
    }

    pub fn times(&self) -> &ScrubberTimes {
        &self.imp().times
    }

    pub fn transport(&self) -> &MediaTransport {
        &self.imp().transport
    }

    pub fn set_title(&self, text: &str) {
        self.meta().set_title(text);
    }

    pub fn set_subtitle(&self, text: &str) {
        self.meta().set_subtitle(text);
    }

    pub fn set_artwork(&self, paintable: Option<&gdk::Texture>) {
        self.artwork().set_paintable(paintable);
    }

    pub fn set_progress(&self, position_seconds: f64, length_seconds: f64) {
        self.scrubber()
            .set_progress(position_seconds, length_seconds);
    }

    pub fn set_seekable(&self, seekable: bool) {
        self.scrubber().set_seekable(seekable);
    }

    pub fn set_position_text(&self, text: &str) {
        self.times().set_position_text(text);
    }

    pub fn set_length_text(&self, text: &str) {
        self.times().set_length_text(text);
    }

    pub fn set_play_state(&self, state: PlayState) {
        self.transport().set_play_state(state);
    }

    pub fn set_can_previous(&self, can: bool) {
        self.transport().set_can_previous(can);
    }

    pub fn set_can_play_pause(&self, can: bool) {
        self.transport().set_can_play_pause(can);
    }

    pub fn set_can_next(&self, can: bool) {
        self.transport().set_can_next(can);
    }
}

impl Default for NowPlayingCard {
    fn default() -> Self {
        Self::new()
    }
}
