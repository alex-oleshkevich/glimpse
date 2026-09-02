use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::OnceLock;

use crate::{set_css_class, set_play_pause};

const REPEAT_ICON: &str = "media-playlist-repeat-symbolic";
const REPEAT_TRACK_ICON: &str = "media-playlist-repeat-song-symbolic";
const ENGAGED: &str = "transport--on";

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "TransportAction")]
pub enum TransportAction {
    #[default]
    PlayPause,
    Previous,
    Next,
    Shuffle,
    Repeat,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "TransportRepeat")]
pub enum Repeat {
    #[default]
    Off,
    Playlist,
    Track,
}

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Transport)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/transport.ui")]
pub struct Transport {
    #[template_child]
    pub shuffle: TemplateChild<gtk4::Button>,
    #[template_child]
    pub previous: TemplateChild<gtk4::Button>,
    #[template_child]
    pub play: TemplateChild<gtk4::Button>,
    #[template_child]
    pub next: TemplateChild<gtk4::Button>,
    #[template_child]
    pub repeat: TemplateChild<gtk4::Button>,

    #[property(name = "playing", get = Self::playing, set = Self::set_playing)]
    playing: Cell<bool>,
    #[property(name = "can-previous", get = Self::can_previous, set = Self::set_can_previous, default = true)]
    can_previous: PhantomData<bool>,
    #[property(name = "can-next", get = Self::can_next, set = Self::set_can_next, default = true)]
    can_next: PhantomData<bool>,
    #[property(name = "can-play", get = Self::can_play, set = Self::set_can_play, default = true)]
    can_play: PhantomData<bool>,
    #[property(name = "can-shuffle", get = Self::can_shuffle, set = Self::set_can_shuffle)]
    can_shuffle: PhantomData<bool>,
    #[property(name = "can-repeat", get = Self::can_repeat, set = Self::set_can_repeat)]
    can_repeat: PhantomData<bool>,
    #[property(name = "shuffle", get = Self::shuffle, set = Self::set_shuffle)]
    shuffle_flag: PhantomData<bool>,
    #[property(name = "repeat", get = Self::repeat, set = Self::set_repeat, builder(Repeat::Off))]
    repeat_mode: Cell<Repeat>,
}

impl Transport {
    fn playing(&self) -> bool {
        self.playing.get()
    }

    fn set_playing(&self, playing: bool) {
        if self.playing.replace(playing) == playing {
            return;
        }
        set_play_pause(&self.play, playing);
    }

    fn can_previous(&self) -> bool {
        self.previous.get_sensitive()
    }

    fn set_can_previous(&self, enabled: bool) {
        self.previous.set_sensitive(enabled);
    }

    fn can_next(&self) -> bool {
        self.next.get_sensitive()
    }

    fn set_can_next(&self, enabled: bool) {
        self.next.set_sensitive(enabled);
    }

    fn can_play(&self) -> bool {
        self.play.get_sensitive()
    }

    fn set_can_play(&self, enabled: bool) {
        self.play.set_sensitive(enabled);
    }

    fn can_shuffle(&self) -> bool {
        self.shuffle.get_visible()
    }

    fn set_can_shuffle(&self, supported: bool) {
        self.shuffle.set_visible(supported);
    }

    fn can_repeat(&self) -> bool {
        self.repeat.get_visible()
    }

    fn set_can_repeat(&self, supported: bool) {
        self.repeat.set_visible(supported);
    }

    fn shuffle(&self) -> bool {
        self.shuffle.has_css_class(ENGAGED)
    }

    fn set_shuffle(&self, shuffle: bool) {
        set_css_class(&*self.shuffle, ENGAGED, shuffle);
    }

    fn repeat(&self) -> Repeat {
        self.repeat_mode.get()
    }

    fn set_repeat(&self, repeat: Repeat) {
        if self.repeat_mode.replace(repeat) == repeat {
            return;
        }
        self.repeat.set_icon_name(match repeat {
            Repeat::Track => REPEAT_TRACK_ICON,
            _ => REPEAT_ICON,
        });
        set_css_class(&*self.repeat, ENGAGED, repeat != Repeat::Off);
    }

    fn emit(&self, action: TransportAction) {
        self.obj().emit_by_name::<()>("action", &[&action]);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Transport {
    const NAME: &'static str = "Transport";
    type Type = super::Transport;
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
impl ObjectImpl for Transport {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("action")
                    .param_types([TransportAction::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj().clone();

        for (button, action) in [
            (&self.shuffle, TransportAction::Shuffle),
            (&self.previous, TransportAction::Previous),
            (&self.play, TransportAction::PlayPause),
            (&self.next, TransportAction::Next),
            (&self.repeat, TransportAction::Repeat),
        ] {
            button.connect_clicked(glib::clone!(
                #[weak]
                obj,
                move |_| obj.imp().emit(action)
            ));
        }
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Transport {}
