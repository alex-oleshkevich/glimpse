mod imp;

use gtk4::{accessible, glib, glib::translate::ToGlibPtr, prelude::*, subclass::prelude::*};
use std::ffi::CStr;
use std::time::Duration;
use zeroize::Zeroizing;

use crate::{set_css_class, set_text_capped};

const USER_MAX_CHARS: usize = 64;
const MESSAGE_MAX_CHARS: usize = 120;
const ERROR: &str = "password-prompt__message--error";
const SHAKE: &str = "password-prompt--shake";
const SHAKE_FOR: Duration = Duration::from_millis(400);

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum MessageKind {
    #[default]
    Info,
    Error,
}

glib::wrapper! {
    pub struct PasswordPrompt(ObjectSubclass<imp::PasswordPrompt>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PasswordPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordPrompt {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_user(&self, user: Option<&str>) {
        set_text_capped(&self.imp().user, user, USER_MAX_CHARS);
    }

    pub fn set_busy(&self, busy: bool) {
        let imp = self.imp();
        if imp.busy.replace(busy) == busy {
            return;
        }
        if busy {
            imp.conceal();
        }
        imp.spinner.set_visible(busy);
        imp.spinner.set_spinning(busy);
        imp.entry.set_show_peek_icon(!busy);
        imp.entry.update_state(&[accessible::State::Busy(busy)]);
        imp.sync_editable();
    }

    pub fn set_message(&self, text: Option<&str>, kind: MessageKind) {
        let message = &self.imp().message;
        let text = glimpse_utils::clean(text.unwrap_or_default(), MESSAGE_MAX_CHARS);
        let error = kind == MessageKind::Error && !text.is_empty();
        if message.text().as_str() != text {
            message.set_text(&text);
            if error {
                self.announce(&text, gtk4::AccessibleAnnouncementPriority::High);
            }
        }
        if message.has_css_class(ERROR) != error {
            set_css_class(&**message, ERROR, error);
        }
    }

    pub fn set_caps_lock(&self, on: bool) {
        let caps = &self.imp().caps;
        if caps.is_child_visible() != on {
            caps.set_child_visible(on);
        }
    }

    pub fn set_available(&self, available: bool) {
        let imp = self.imp();
        let unavailable = !available;
        if imp.unavailable.replace(unavailable) == unavailable {
            return;
        }
        if unavailable {
            imp.release();
        }
        imp.entry_row.set_child_visible(available);
        imp.sync_editable();
    }

    pub fn set_interactive(&self, interactive: bool) {
        let imp = self.imp();
        let mirrored = !interactive;
        if imp.mirrored.replace(mirrored) == mirrored {
            return;
        }
        if mirrored {
            imp.release();
        }
        imp.entry.set_can_focus(interactive);
        imp.entry.set_can_target(interactive);
        imp.sync_editable();
    }

    pub fn shake(&self) {
        let imp = self.imp();
        if imp.shaking.replace(true) {
            return;
        }
        self.add_css_class(SHAKE);
        let prompt = self.downgrade();
        glib::timeout_add_local_once(SHAKE_FOR, move || {
            if let Some(prompt) = prompt.upgrade() {
                prompt.remove_css_class(SHAKE);
                prompt.imp().shaking.set(false);
            }
        });
    }

    pub fn take_text(&self) -> Zeroizing<String> {
        let imp = self.imp();
        let editable: &gtk4::Editable = imp.entry.upcast_ref();
        let raw = unsafe { gtk4::ffi::gtk_editable_get_text(editable.to_glib_none().0) };
        let text = match raw.is_null() {
            true => Zeroizing::new(String::new()),
            false => Zeroizing::new(
                unsafe { CStr::from_ptr(raw) }
                    .to_string_lossy()
                    .into_owned(),
            ),
        };
        imp.entry.set_text("");
        imp.conceal();
        text
    }

    pub fn connect_submitted<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "submitted",
            false,
            glib::closure_local!(move |prompt: Self| f(&prompt)),
        )
    }

    pub fn connect_edited<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "edited",
            false,
            glib::closure_local!(move |prompt: Self| f(&prompt)),
        )
    }
}

#[cfg(test)]
mod tests;
