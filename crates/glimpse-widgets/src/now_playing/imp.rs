use gtk4::{
    CompositeTemplate, TemplateChild, accessible, gdk, glib, prelude::*, subclass::prelude::*,
};
use std::marker::PhantomData;

use crate::{Scrubber, Transport, set_css_class, set_text};

const EMPTY_ART_ICON: &str = "audio-x-generic-symbolic";
const EMPTY_ART: &str = "now-playing__art--empty";

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::NowPlaying)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/now_playing.ui")]
pub struct NowPlaying {
    #[template_child]
    pub art: TemplateChild<gtk4::Image>,
    #[template_child]
    pub source_line: TemplateChild<gtk4::Box>,
    #[template_child]
    pub source_icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub source: TemplateChild<gtk4::Label>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub artist: TemplateChild<gtk4::Label>,
    #[template_child]
    pub album: TemplateChild<gtk4::Label>,
    #[template_child]
    pub scrubber: TemplateChild<Scrubber>,
    #[template_child]
    pub transport: TemplateChild<Transport>,

    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
    #[property(name = "source", get = Self::source, set = Self::set_source, nullable)]
    source_text: PhantomData<Option<String>>,
    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title_text: PhantomData<Option<String>>,
    #[property(name = "artist", get = Self::artist, set = Self::set_artist, nullable)]
    artist_text: PhantomData<Option<String>>,
    #[property(name = "album", get = Self::album, set = Self::set_album, nullable)]
    album_text: PhantomData<Option<String>>,
}

impl NowPlaying {
    fn icon_name(&self) -> Option<String> {
        self.source_icon.icon_name().map(|name| name.to_string())
    }

    fn set_icon_name(&self, name: Option<String>) {
        if self.icon_name() == name {
            return;
        }
        self.source_icon.set_icon_name(name.as_deref());
        self.source_icon.set_visible(name.is_some());
        self.sync_source_line();
    }

    fn source(&self) -> Option<String> {
        self.source
            .get_visible()
            .then(|| self.source.text().to_string())
    }

    fn set_source(&self, source: Option<String>) {
        set_text(&self.source, source.as_deref());
        self.sync_source_line();
    }

    fn sync_source_line(&self) {
        self.source_line
            .set_visible(self.source_icon.get_visible() || self.source.get_visible());
    }

    fn title(&self) -> Option<String> {
        self.title
            .get_visible()
            .then(|| self.title.text().to_string())
    }

    fn set_title(&self, title: Option<String>) {
        set_text(&self.title, title.as_deref());
        self.obj()
            .update_property(&[accessible::Property::Label(self.title.text().as_str())]);
    }

    fn artist(&self) -> Option<String> {
        self.artist
            .get_visible()
            .then(|| self.artist.text().to_string())
    }

    fn set_artist(&self, artist: Option<String>) {
        set_text(&self.artist, artist.as_deref());
    }

    fn album(&self) -> Option<String> {
        self.album
            .get_visible()
            .then(|| self.album.text().to_string())
    }

    fn set_album(&self, album: Option<String>) {
        set_text(&self.album, album.as_deref());
    }

    pub(super) fn set_art(&self, art: Option<&gdk::Paintable>) {
        if self.art.paintable().as_ref() == art {
            return;
        }
        match art {
            Some(paintable) => self.art.set_paintable(Some(paintable)),
            None => self.art.set_icon_name(Some(EMPTY_ART_ICON)),
        }
        set_css_class(&*self.art, EMPTY_ART, art.is_none());
    }
}

#[glib::object_subclass]
impl ObjectSubclass for NowPlaying {
    const NAME: &'static str = "NowPlaying";
    type Type = super::NowPlaying;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for NowPlaying {
    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for NowPlaying {}
