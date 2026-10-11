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
