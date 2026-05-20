mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct Tile(ObjectSubclass<imp::Tile>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl Tile {
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

    pub fn set_right(&self, child: Option<impl IsA<gtk4::Widget>>) {
        let slot = &self.imp().right_slot;
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

    pub fn set_activatable(&self, v: bool) {
        self.imp().activatable.set(v);
        if v {
            self.add_css_class("activatable");
        } else {
            self.remove_css_class("activatable");
        }
    }

    pub fn connect_activated(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.set_activatable(true);
        self.connect_closure(
            "activated",
            false,
            closure_local!(move |tile: &Self| f(tile)),
        )
    }
}

impl Default for Tile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn tile_has_css_class() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = Tile::new();
        assert!(tile.has_css_class("tile"));
    }

    #[test]
    fn secondary_label_hidden_by_default() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = Tile::new();
        assert!(!tile.imp().secondary_label.is_visible());
    }

    #[test]
    fn set_secondary_shows_and_hides_label() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = Tile::new();
        tile.set_secondary(Some("detail"));
        assert!(tile.imp().secondary_label.is_visible());
        assert_eq!(tile.imp().secondary_label.text(), "detail");
        tile.set_secondary(None);
        assert!(!tile.imp().secondary_label.is_visible());
    }

    #[test]
    fn slots_hidden_until_populated() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = Tile::new();
        assert!(!tile.imp().left_slot.is_visible());
        assert!(!tile.imp().right_slot.is_visible());
    }

    #[test]
    fn set_activatable_toggles_css_class() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = Tile::new();
        assert!(!tile.has_css_class("activatable"));
        tile.set_activatable(true);
        assert!(tile.has_css_class("activatable"));
        tile.set_activatable(false);
        assert!(!tile.has_css_class("activatable"));
    }
}
