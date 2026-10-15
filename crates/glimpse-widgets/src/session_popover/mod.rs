mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::Row;

pub const LOCK: &str = "lock";
pub const SUSPEND: &str = "suspend";
pub const HIBERNATE: &str = "hibernate";
pub const LOG_OUT: &str = "logout";
pub const REBOOT: &str = "reboot";
pub const POWER_OFF: &str = "power-off";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionState {
    pub visible: bool,
    pub enabled: bool,
    pub subtitle: Option<String>,
}

glib::wrapper! {
    pub struct SessionPopover(ObjectSubclass<imp::SessionPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SessionPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_heading(&self, user: Option<&str>, signed_in: Option<&str>) {
        let shell = &self.imp().shell;
        shell.set_title(user);
        shell.set_subtitle(signed_in);
    }

    pub fn set_action(&self, action: &str, state: &ActionState) {
        let row = match action {
            LOCK => &self.imp().lock,
            SUSPEND => &self.imp().suspend,
            HIBERNATE => &self.imp().hibernate,
            LOG_OUT => &self.imp().logout,
            REBOOT => &self.imp().reboot,
            POWER_OFF => &self.imp().power_off,
            _ => return,
        };
        let row = row.upcast_ref::<Row>();
        if row.get_visible() == state.visible
            && row.is_sensitive() == state.enabled
            && row.subtitle().as_deref() == state.subtitle.as_deref()
        {
            return;
        }
        row.set_visible(state.visible);
        row.set_sensitive(state.enabled);
        row.set_subtitle(state.subtitle.as_deref());
    }

    pub fn connect_action_requested<F: Fn(&Self, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "action-requested",
            false,
            glib::closure_local!(move |popover: Self, action: String| f(&popover, &action)),
        )
    }
}
