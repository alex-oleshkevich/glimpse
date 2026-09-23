use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};
use std::cell::Cell;
use std::sync::OnceLock;

use crate::TransportAction;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/track_card.ui")]
pub struct TrackCard {
    #[template_child]
    pub body: TemplateChild<gtk4::Box>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub artist: TemplateChild<gtk4::Label>,
    #[template_child]
    pub play: TemplateChild<gtk4::Button>,
    #[template_child]
    pub next: TemplateChild<gtk4::Button>,
    pub playing: Cell<bool>,
}

impl TrackCard {
    fn emit(&self, action: TransportAction) {
        self.obj().emit_by_name::<()>("action", &[&action]);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for TrackCard {
    const NAME: &'static str = "TrackCard";
    type Type = super::TrackCard;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for TrackCard {
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
        let card = self.obj();

        self.play.connect_clicked(glib::clone!(
            #[weak]
            card,
            move |button| {
                if !button.is_sensitive() {
                    return;
                }
                card.imp().emit(TransportAction::PlayPause);
            }
        ));
        self.next.connect_clicked(glib::clone!(
            #[weak]
            card,
            move |button| {
                if !button.is_sensitive() {
                    return;
                }
                card.imp().emit(TransportAction::Next);
            }
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for TrackCard {}
