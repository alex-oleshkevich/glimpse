use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::sync::OnceLock;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::PlayerRow)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/player_row.ui")]
    pub struct PlayerRow {
        #[template_child]
        pub row: TemplateChild<crate::ActionRow>,
        #[template_child]
        pub toggle: TemplateChild<gtk4::Button>,

        #[property(name = "playing", get = Self::playing, set = Self::set_playing)]
        playing: Cell<bool>,
    }

    impl PlayerRow {
        fn playing(&self) -> bool {
            self.playing.get()
        }

        fn set_playing(&self, playing: bool) {
            if self.playing.replace(playing) == playing {
                return;
            }
            crate::set_play_pause(&self.toggle, playing);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlayerRow {
        const NAME: &'static str = "PlayerRow";
        type Type = super::PlayerRow;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            crate::ActionRow::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for PlayerRow {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("activated").build(),
                    glib::subclass::Signal::builder("toggled").build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj().clone();
            self.toggle.connect_clicked(glib::clone!(
                #[weak]
                obj,
                move |_| obj.emit_by_name::<()>("toggled", &[])
            ));
            self.row.connect_activated(glib::clone!(
                #[weak]
                obj,
                move |_| obj.emit_by_name::<()>("activated", &[])
            ));
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for PlayerRow {}
}

glib::wrapper! {
    pub struct PlayerRow(ObjectSubclass<imp::PlayerRow>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PlayerRow {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_title(&self, title: Option<&str>) {
        self.imp().row.upcast_ref::<crate::Row>().set_title(title);
    }

    pub fn set_subtitle(&self, subtitle: Option<&str>) {
        self.imp()
            .row
            .upcast_ref::<crate::Row>()
            .set_subtitle(subtitle);
    }

    pub fn set_icon_name(&self, icon_name: Option<&str>) {
        self.imp()
            .row
            .upcast_ref::<crate::Row>()
            .set_icon_name(icon_name);
    }

    pub fn connect_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |row: Self| f(&row)),
        )
    }

    pub fn connect_toggled<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            glib::closure_local!(move |row: Self| f(&row)),
        )
    }
}
