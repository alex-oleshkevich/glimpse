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
        type ParentType = crate::Row;

        fn class_init(klass: &mut Self::Class) {
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
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("toggled").build()])
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj().clone();
            self.toggle.connect_clicked(glib::clone!(
                #[weak]
                obj,
                move |_| obj.emit_by_name::<()>("toggled", &[])
            ));
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for PlayerRow {}
    impl ButtonImpl for PlayerRow {}
    impl crate::row::RowImpl for PlayerRow {}
}

glib::wrapper! {
    pub struct PlayerRow(ObjectSubclass<imp::PlayerRow>)
        @extends crate::Row, gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
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

    pub fn connect_toggled<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            glib::closure_local!(move |row: Self| f(&row)),
        )
    }
}
