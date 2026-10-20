mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{ActionRow, NowPlaying, Player};

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

    pub fn player(&self) -> NowPlaying {
        self.imp().player.clone()
    }

    pub fn open_player(&self) -> ActionRow {
        self.imp().open_player.clone()
    }

    pub fn set_others(&self, players: Option<&[Player]>) {
        let imp = self.imp();
        let players = players.unwrap_or_default();
        imp.others.set_visible(!players.is_empty());
        imp.list.set_players(players);
    }

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
