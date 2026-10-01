mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::TransportAction;

glib::wrapper! {
    pub struct TrackCard(ObjectSubclass<imp::TrackCard>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for TrackCard {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub can_play_pause: bool,
    pub can_next: bool,
}

impl TrackCard {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_track(&self, track: Option<&Track>) {
        let imp = self.imp();
        let Some(track) = track else {
            if !self.get_visible() {
                return;
            }
            self.set_visible(false);
            return;
        };

        self.set_visible(true);
        set_cleaned(&imp.title, &track.title, crate::TEXT_MAX_CHARS);
        set_cleaned(&imp.artist, &track.artist, crate::TEXT_MAX_CHARS);

        if imp.playing.replace(track.playing) != track.playing {
            crate::set_play_pause(&imp.play, track.playing);
        }
        if imp.play.is_sensitive() != track.can_play_pause {
            imp.play.set_sensitive(track.can_play_pause);
        }
        if imp.next.is_sensitive() != track.can_next {
            imp.next.set_sensitive(track.can_next);
        }
    }

    pub fn connect_action<F: Fn(&Self, TransportAction) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "action",
            false,
            glib::closure_local!(move |card: Self, action: TransportAction| f(&card, action)),
        )
    }
}

fn set_cleaned(label: &gtk4::Label, text: &str, cap: usize) {
    let cleaned = glimpse_utils::clean(text, cap);
    if label.text().as_str() == cleaned {
        return;
    }
    label.set_text(&cleaned);
    label.set_visible(!cleaned.is_empty());
}

#[cfg(test)]
mod tests;
