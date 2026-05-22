use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::widgets::{
    media_artwork::MediaArtwork, media_meta::MediaMeta, media_scrubber::MediaScrubber,
    media_transport::MediaTransport, scrubber_times::ScrubberTimes,
};

#[derive(Default)]
pub struct NowPlayingCard {
    pub artwork: MediaArtwork,
    pub meta: MediaMeta,
    pub scrubber: MediaScrubber,
    pub times: ScrubberTimes,
    pub transport: MediaTransport,
}

#[glib::object_subclass]
impl ObjectSubclass for NowPlayingCard {
    const NAME: &'static str = "NowPlayingCard";
    type Type = super::NowPlayingCard;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for NowPlayingCard {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj();
        obj.set_orientation(gtk4::Orientation::Vertical);
        obj.set_spacing(12);
        obj.add_css_class("mpris-card");

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        header.add_css_class("mpris-card__header");
        self.artwork.add_css_class("mpris-card__artwork");
        self.meta.add_css_class("mpris-card__meta");
        header.append(&self.artwork);
        header.append(&self.meta);
        obj.append(&header);

        self.scrubber.add_css_class("mpris-card__scrubber");
        obj.append(&self.scrubber);

        self.times.add_css_class("mpris-card__times");
        obj.append(&self.times);

        self.transport.add_css_class("mpris-card__transport");
        obj.append(&self.transport);
    }
}

impl WidgetImpl for NowPlayingCard {}
impl BoxImpl for NowPlayingCard {}
