mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::SessionActionState;

glib::wrapper! {
    pub struct SessionSheet(ObjectSubclass<imp::SessionSheet>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SessionSheet {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionSheet {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_action(&self, action: &str, state: &SessionActionState) {
        let imp = self.imp();
        let row = match action {
            crate::SUSPEND => &imp.suspend,
            crate::REBOOT => &imp.reboot,
            crate::POWER_OFF => &imp.power_off,
            other => {
                tracing::warn!(action = other, "unknown session action");
                return;
            }
        };
        if row.get_visible() == state.visible
            && row.is_sensitive() == state.enabled
            && row.subtitle().as_deref() == state.subtitle.as_deref()
        {
            return;
        }
        row.set_visible(state.visible);
        row.set_sensitive(state.enabled);
        row.set_subtitle(state.subtitle.as_deref());
        imp.close_confirm_if_unavailable(action, state);
    }

    pub fn set_error(&self, text: Option<&str>) {
        let imp = self.imp();
        if !imp.revealer.reveals_child() {
            return;
        }
        let error = &imp.error;
        let flat = glimpse_utils::clean(text.unwrap_or_default(), crate::TEXT_MAX_CHARS);
        if error.text().as_str() != flat {
            error.set_text(&flat);
            error.set_visible(!flat.is_empty());
        }
        if !flat.is_empty() {
            imp.return_to_menu_and_focus();
            self.announce(&flat, gtk4::AccessibleAnnouncementPriority::High);
        }
    }

    pub fn toggle(&self) {
        let imp = self.imp();
        if !imp.revealer.reveals_child() && !imp.has_actions() {
            return;
        }
        crate::drawer::set(&imp.revealer, !imp.revealer.reveals_child());
    }

    pub fn close(&self) {
        crate::drawer::set(&self.imp().revealer, false);
    }

    pub fn is_open(&self) -> bool {
        self.imp().revealer.reveals_child()
    }

    pub fn has_actions(&self) -> bool {
        self.imp().has_actions()
    }

    pub fn connect_action_requested<F: Fn(&Self, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "action-requested",
            false,
            glib::closure_local!(move |sheet: Self, action: String| f(&sheet, &action)),
        )
    }

    pub fn connect_closed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "closed",
            false,
            glib::closure_local!(move |sheet: Self| f(&sheet)),
        )
    }
}

#[cfg(test)]
mod tests;
