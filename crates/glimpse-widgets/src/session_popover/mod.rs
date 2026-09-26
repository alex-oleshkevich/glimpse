mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

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
        let imp = self.imp();
        imp.hero.set_icon_name(Some("avatar-default-symbolic"));
        imp.hero.set_title(user);
        imp.hero.set_subtitle(signed_in);
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

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
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

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }
}
