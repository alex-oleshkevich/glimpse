use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

use crate::widgets::{media_artwork::MediaArtwork, media_meta::MediaMeta};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/secondary_player_row.ui")]
pub struct SecondaryPlayerRow {
    #[template_child]
    pub artwork: TemplateChild<MediaArtwork>,
    #[template_child]
    pub meta: TemplateChild<MediaMeta>,
    #[template_child]
    pub play_pause: TemplateChild<gtk4::Button>,
    #[template_child]
    pub next: TemplateChild<gtk4::Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for SecondaryPlayerRow {
    const NAME: &'static str = "SecondaryPlayerRow";
    type Type = super::SecondaryPlayerRow;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for SecondaryPlayerRow {
    fn constructed(&self) {
        self.parent_constructed();

        let obj_weak = self.obj().downgrade();
        self.play_pause.connect_clicked({
            let obj_weak = obj_weak.clone();
            move |_| {
                if let Some(row) = obj_weak.upgrade() {
                    row.emit_by_name::<()>("play-pause-clicked", &[]);
                }
            }
        });
        self.next.connect_clicked({
            let obj_weak = obj_weak.clone();
            move |_| {
                if let Some(row) = obj_weak.upgrade() {
                    row.emit_by_name::<()>("next-clicked", &[]);
                }
            }
        });

        let artwork_obj = obj_weak.clone();
        self.artwork.connect_activated(move |_| {
            if let Some(row) = artwork_obj.upgrade() {
                row.emit_by_name::<()>("activated", &[]);
            }
        });
        let meta_obj = obj_weak;
        self.meta.connect_activated(move |_| {
            if let Some(row) = meta_obj.upgrade() {
                row.emit_by_name::<()>("activated", &[]);
            }
        });
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("play-pause-clicked").build(),
                Signal::builder("next-clicked").build(),
                Signal::builder("activated").build(),
            ]
        })
    }
}

impl WidgetImpl for SecondaryPlayerRow {}
impl BoxImpl for SecondaryPlayerRow {}
