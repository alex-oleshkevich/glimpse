mod imp;

use gtk4::{gdk, glib, subclass::prelude::*};

use crate::widgets::{
    media_artwork::MediaArtwork, media_meta::MediaMeta, media_scrubber::MediaScrubber,
    media_transport::MediaTransport, scrubber_times::ScrubberTimes,
};

glib::wrapper! {
    pub struct NowPlayingCard(ObjectSubclass<imp::NowPlayingCard>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl NowPlayingCard {
    pub fn new() -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.imp().artwork.set_artwork_size(48);
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
}

impl Default for NowPlayingCard {
    fn default() -> Self {
        Self::new()
    }
}
