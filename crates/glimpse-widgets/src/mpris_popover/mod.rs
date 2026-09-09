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

    /// `None` is the section switched off, which is not the same as an empty one: an empty section
    /// shows its placeholder, and "Nothing else playing" is the opposite of what a viewer who
    /// turned the list off asked for.
    pub fn set_others(&self, players: Option<&[Player]>) {
        let imp = self.imp();
        imp.others.set_visible(players.is_some());

        let Some(players) = players else {
            return;
        };
        imp.others.set_empty(players.is_empty());
        imp.list.set_players(players);
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
