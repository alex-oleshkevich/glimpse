use glib::subclass::Signal;
use gtk4::{glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

use crate::widgets::{media_artwork::MediaArtwork, media_meta::MediaMeta};

#[derive(Default)]
pub struct SecondaryPlayerRow {
    pub artwork: MediaArtwork,
    pub meta: MediaMeta,
    pub play_pause: gtk4::Button,
    pub next: gtk4::Button,
}

#[glib::object_subclass]
impl ObjectSubclass for SecondaryPlayerRow {
    const NAME: &'static str = "SecondaryPlayerRow";
    type Type = super::SecondaryPlayerRow;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for SecondaryPlayerRow {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj();
        obj.set_orientation(gtk4::Orientation::Horizontal);
        obj.set_spacing(10);
        obj.add_css_class("mpris-row");

        self.artwork.add_css_class("mpris-row__artwork");
        obj.append(&self.artwork);

        self.meta.add_css_class("mpris-row__meta");
        obj.append(&self.meta);

        let trailing = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        trailing.set_valign(gtk4::Align::Center);
        trailing.add_css_class("mpris-row__trailing");

        self.play_pause
            .set_icon_name("media-playback-start-symbolic");
        self.play_pause.add_css_class("flat");
        self.play_pause.add_css_class("circular");
        self.play_pause.add_css_class("mpris-row__button");
        self.play_pause.add_css_class("mpris-row__play");
        trailing.append(&self.play_pause);

        self.next.set_icon_name("media-skip-forward-symbolic");
        self.next.add_css_class("flat");
        self.next.add_css_class("circular");
        self.next.add_css_class("mpris-row__button");
        self.next.add_css_class("mpris-row__next");
        trailing.append(&self.next);

        obj.append(&trailing);

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
