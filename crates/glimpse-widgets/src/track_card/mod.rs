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
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn track(
        title: &str,
        artist: &str,
        playing: bool,
        can_play_pause: bool,
        can_next: bool,
    ) -> Track {
        Track {
            title: title.to_owned(),
            artist: artist.to_owned(),
            playing,
            can_play_pause,
            can_next,
        }
    }

    #[test]
    #[ignore = "needs a display"]
    fn track_card_states() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");
        let _styles = crate::Styles::install(adw::ColorScheme::Default);

        let card = TrackCard::new();
        let window = gtk4::Window::new();
        window.set_child(Some(&card));
        let imp = card.imp();

        assert!(!card.get_visible(), "an untouched card starts hidden");

        card.set_track(None);
        assert!(
            !card.get_visible(),
            "None keeps an already-hidden card hidden"
        );

        let actions = Rc::new(RefCell::new(Vec::new()));
        card.connect_action({
            let actions = Rc::clone(&actions);
            move |_, action| actions.borrow_mut().push(action)
        });

        let icon_notifies = Rc::new(Cell::new(0));
        imp.play.connect_notify_local(Some("icon-name"), {
            let icon_notifies = Rc::clone(&icon_notifies);
            move |_, _| icon_notifies.set(icon_notifies.get() + 1)
        });

        card.set_track(Some(&track(
            "Weightless",
            "Marconi Union",
            true,
            true,
            true,
        )));
        assert!(card.get_visible(), "a track shows the card");
        assert_eq!(imp.title.text().as_str(), "Weightless");
        assert_eq!(imp.artist.text().as_str(), "Marconi Union");
        assert!(imp.artist.get_visible(), "a non-empty artist shows");
        assert!(imp.play.is_sensitive());
        assert!(imp.next.is_sensitive());

        card.set_track(Some(&track("W", "A", true, true, true)));
        let short_width = card.measure(gtk4::Orientation::Horizontal, -1).1;
        card.set_track(Some(&track(
            "A Genuinely Very Long Track Title Indeed",
            "An Equally Long And Wordy Artist Name",
            true,
            true,
            true,
        )));
        let long_width = card.measure(gtk4::Orientation::Horizontal, -1).1;
        assert_eq!(
            short_width, long_width,
            "neither a long title nor a long artist changes the card width"
        );

        card.set_track(Some(&track("", "", true, true, true)));
        assert!(
            !imp.title.get_visible(),
            "an empty title never leaves a blank line"
        );
        assert!(!imp.artist.get_visible(), "an empty artist hides its line");

        card.set_track(Some(&track(
            "Lunch\u{202e}gpj.exe",
            "Marconi Union",
            true,
            true,
            true,
        )));
        assert!(
            !imp.title.text().contains('\u{202e}'),
            "a bidi override does not reach the title"
        );

        card.set_track(Some(&track(
            "Weightless",
            "Marconi Union",
            false,
            false,
            false,
        )));
        assert!(
            !imp.play.is_sensitive(),
            "can_play_pause off leaves play insensitive"
        );
        assert!(
            !imp.next.is_sensitive(),
            "can_next off leaves next insensitive"
        );

        imp.play.emit_clicked();
        imp.next.emit_clicked();
        assert!(
            actions.borrow().is_empty(),
            "an insensitive button emits nothing, even clicked directly"
        );

        card.set_track(Some(&track(
            "Weightless",
            "Marconi Union",
            false,
            true,
            true,
        )));
        imp.play.emit_clicked();
        imp.next.emit_clicked();
        assert_eq!(
            *actions.borrow(),
            [TransportAction::PlayPause, TransportAction::Next],
            "each capability emits its own action exactly once"
        );

        let before_repeat = icon_notifies.get();
        card.set_track(Some(&track(
            "Weightless",
            "Marconi Union",
            false,
            true,
            true,
        )));
        assert_eq!(
            icon_notifies.get(),
            before_repeat,
            "setting an identical Track does not touch the play icon again"
        );

        let hostile_title = format!("{}{}", "ё\t\n ".repeat(3), "ё".repeat(200));
        card.set_track(Some(&track(
            &hostile_title,
            "  Ma  rconi\nUnion  ",
            true,
            true,
            true,
        )));
        assert_eq!(
            imp.title.text().chars().count(),
            crate::TEXT_MAX_CHARS + 1,
            "a hostile title is capped by characters, with a trailing ellipsis"
        );
        assert!(
            !imp.title.text().contains('\n'),
            "the title is flattened to one line before it is capped"
        );
        assert_eq!(
            imp.artist.text().as_str(),
            "Ma rconi Union",
            "the artist is flattened to single spaces"
        );

        card.set_track(None);
        assert!(!card.get_visible(), "None hides a shown card");

        window.destroy();
    }
}
