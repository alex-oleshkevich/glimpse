mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{NowPlaying, Player};

glib::wrapper! {
    pub struct MprisPopover(ObjectSubclass<imp::MprisPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for MprisPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl MprisPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The primary player itself, so a caller reaches its `Scrubber` and `Transport` rather than
    /// this popover growing a setter per control. It is a whole widget on its own — the lock
    /// screen is its second reader — and nothing here reimplements a part of it.
    pub fn player(&self) -> NowPlaying {
        self.imp().player.clone()
    }

    pub fn set_others(&self, players: Option<&[Player]>) {
        let imp = self.imp();
        let players = players.unwrap_or_default();
        imp.others.set_visible(!players.is_empty());
        imp.list.set_players(players);
    }

    pub fn set_volume(&self, volume: Option<f64>) {
        let fader = &self.imp().volume;
        if fader.get_visible() != volume.is_some() {
            fader.set_visible(volume.is_some());
        }
        if let Some(volume) = volume.filter(|volume| volume.is_finite()) {
            let value = (volume * 100.0).clamp(0.0, 100.0);
            if fader.value() != value {
                fader.set_value(value);
            }
        }
    }

    pub fn connect_volume_changed<F: Fn(&Self, f64) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "volume-changed",
            false,
            glib::closure_local!(move |popover: Self, volume: f64| handler(&popover, volume)),
        )
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| handler(&popover)),
        )
    }

    /// Both carry the row's key rather than a captured widget or its position: a row is reused in
    /// place whenever the list changes, so neither survives the next `set_others`.
    pub fn connect_raise_requested<F: Fn(&Self, String) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "raise-requested",
            false,
            glib::closure_local!(move |popover: Self, key: String| handler(&popover, key)),
        )
    }

    pub fn connect_toggle_requested<F: Fn(&Self, String) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggle-requested",
            false,
            glib::closure_local!(move |popover: Self, key: String| handler(&popover, key)),
        )
    }
}
