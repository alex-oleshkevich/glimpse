mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct ExpanderTile(ObjectSubclass<imp::ExpanderTile>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl ExpanderTile {
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

    pub fn set_child(&self, child: Option<impl IsA<gtk4::Widget>>) {
        let slot = &self.imp().child_slot;
        if let Some(w) = slot.first_child() {
            slot.remove(&w);
        }
        if let Some(w) = child {
            slot.append(&w);
        }
    }

    pub fn set_expanded(&self, expanded: bool) {
        self.apply_expanded(expanded);
    }

    pub fn is_expanded(&self) -> bool {
        self.imp().expanded.get()
    }

    pub fn connect_expanded(&self, f: impl Fn(&Self, bool) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "expanded",
            false,
            closure_local!(move |tile: &Self, expanded: bool| f(tile, expanded)),
        )
    }

    pub(super) fn apply_expanded(&self, expanded: bool) {
        let imp = self.imp();
        imp.expanded.set(expanded);
        imp.revealer.set_reveal_child(expanded);
        if expanded {
            imp.chevron.add_css_class("expanded");
        } else {
            imp.chevron.remove_css_class("expanded");
        }
    }
}

impl Default for ExpanderTile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;

    #[test]
    fn expander_tile_has_css_class() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = ExpanderTile::new();
        assert!(tile.has_css_class("expander-tile"));
    }

    #[test]
    fn collapsed_by_default() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = ExpanderTile::new();
        assert!(!tile.is_expanded());
        assert!(!tile.imp().revealer.reveals_child());
    }

    #[test]
    fn set_expanded_updates_revealer_and_chevron() {
        if !gtk_available_on_this_thread() {
            return;
        }
        let tile = ExpanderTile::new();
        tile.set_expanded(true);
        assert!(tile.is_expanded());
        assert!(tile.imp().revealer.reveals_child());
        assert!(tile.imp().chevron.has_css_class("expanded"));
        tile.set_expanded(false);
        assert!(!tile.is_expanded());
        assert!(!tile.imp().revealer.reveals_child());
        assert!(!tile.imp().chevron.has_css_class("expanded"));
    }
}
