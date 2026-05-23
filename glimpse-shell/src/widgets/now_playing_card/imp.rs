use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

use crate::widgets::{
    media_artwork::MediaArtwork, media_meta::MediaMeta, media_scrubber::MediaScrubber,
    media_transport::MediaTransport, scrubber_times::ScrubberTimes,
};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/now_playing_card.ui")]
pub struct NowPlayingCard {
    #[template_child]
    pub header: TemplateChild<gtk4::Box>,
    #[template_child]
    pub artwork: TemplateChild<MediaArtwork>,
    #[template_child]
    pub meta: TemplateChild<MediaMeta>,
    #[template_child]
    pub scrubber: TemplateChild<MediaScrubber>,
    #[template_child]
    pub times: TemplateChild<ScrubberTimes>,
    #[template_child]
    pub transport: TemplateChild<MediaTransport>,
}

#[glib::object_subclass]
impl ObjectSubclass for NowPlayingCard {
    const NAME: &'static str = "NowPlayingCard";
    type Type = super::NowPlayingCard;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for NowPlayingCard {}

impl WidgetImpl for NowPlayingCard {}
impl BoxImpl for NowPlayingCard {}
