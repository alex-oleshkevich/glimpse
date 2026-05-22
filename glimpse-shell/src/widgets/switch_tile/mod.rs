mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct SwitchTile(ObjectSubclass<imp::SwitchTile>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl SwitchTile {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_primary(&self, text: &str) {
        self.imp().primary_label.set_text(text);
    }

    pub fn set_secondary(&self, text: Option<&str>) {
        let label = &self.imp().secondary_label;
        match text {
            Some(t) => {
                label.set_text(t);
                label.set_visible(true);
            }
            None => label.set_visible(false),
        }
    }

    pub fn set_left(&self, child: Option<impl IsA<gtk4::Widget>>) {
        let slot = &self.imp().left_slot;
        if let Some(w) = slot.first_child() {
            slot.remove(&w);
        }
        if let Some(w) = child {
            slot.append(&w);
            slot.set_visible(true);
        } else {
            slot.set_visible(false);
        }
    }

    pub fn set_active(&self, active: bool) {
        let toggle = &self.imp().toggle;
        if toggle.is_active() != active {
            toggle.set_active(active);
        }
        if toggle.state() != active {
            toggle.set_state(active);
        }
    }

    pub fn is_active(&self) -> bool {
        self.imp().toggle.is_active()
    }

    pub fn connect_toggled(&self, f: impl Fn(&Self, bool) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            closure_local!(move |tile: &Self, active: bool| f(tile, active)),
        )
    }
}

impl Default for SwitchTile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;

    #[test]
    fn switch_tile_has_css_classes() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = SwitchTile::new();
        assert!(tile.has_css_class("tile"));
        assert!(tile.has_css_class("switch-tile"));
    }

    #[test]
    fn secondary_label_hidden_by_default() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = SwitchTile::new();
        assert!(!tile.imp().secondary_label.is_visible());
    }

    #[test]
    fn left_slot_hidden_until_populated() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = SwitchTile::new();
        assert!(!tile.imp().left_slot.is_visible());
    }

    #[test]
    fn set_active_reflects_on_toggle() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = SwitchTile::new();
        assert!(!tile.is_active());
        assert!(!tile.imp().toggle.state());
        tile.set_active(true);
        assert!(tile.is_active());
        assert!(tile.imp().toggle.state());
    }
}
