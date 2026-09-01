mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::clear_children;
use crate::indicator::truncate;

glib::wrapper! {
    pub struct Hero(ObjectSubclass<imp::Hero>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

pub(crate) const TEXT_MAX_CHARS: usize = 128;

impl Default for Hero {
    fn default() -> Self {
        Self::new()
    }
}

impl Hero {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_icon(&self, icon: Option<&gio::Icon>) {
        let imp = self.imp();
        if icons_equal(imp.gicon.borrow().as_ref(), icon) {
            return;
        }
        imp.gicon.replace(icon.cloned());
        match icon {
            Some(icon) => imp.icon.set_from_gicon(icon),
            None => imp.icon.clear(),
        }
        imp.icon.set_visible(icon.is_some());
    }

    pub fn set_slot(&self, slot: &impl IsA<gtk4::Widget>) {
        let imp = self.imp();
        self.clear_slot();
        imp.slot.append(slot);
        imp.slot.set_visible(true);
    }

    pub fn clear_slot(&self) {
        let slot = &self.imp().slot;
        clear_children(slot);
        slot.set_visible(false);
    }
}

fn icons_equal(current: Option<&gio::Icon>, next: Option<&gio::Icon>) -> bool {
    match (current, next) {
        (None, None) => true,
        (Some(current), Some(next)) => current.equal(Some(next)),
        _ => false,
    }
}

pub(crate) fn set_text(label: &gtk4::Label, value: Option<&str>) {
    let text = truncate(value.unwrap_or_default(), TEXT_MAX_CHARS);
    if label.text().as_str() == text {
        return;
    }
    label.set_text(&text);
    label.set_visible(!text.is_empty());
}
