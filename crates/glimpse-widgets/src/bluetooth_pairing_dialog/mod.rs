mod imp;

use adw::prelude::*;
use gettextrs::gettext;
use gtk4::glib;
use gtk4::glib::subclass::prelude::*;

use imp::{ACCEPT, NAME_MAX, OK};

pub use imp::{Entry, PASSKEY_MAX, PIN_MAX};

glib::wrapper! {
    pub struct PairingDialog(ObjectSubclass<imp::PairingDialog>)
        @extends adw::AlertDialog, adw::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingAnswer {
    Deny,
    Accept,
    Pin(String),
    Passkey(u32),
}

impl Default for PairingDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl PairingDialog {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn ask(&self, device: &str, name: &str, kind: Entry, code: &str) {
        if self.imp().asked.replace(device.to_owned()) != device {
            self.imp().entry.set_text("");
        }
        let (heading, body) = match kind {
            Entry::Pin => (
                gettext("Enter the PIN"),
                gettext("Type the PIN shown on {device}, then press OK.")
                    .replace("{device}", &capped(name)),
            ),
            Entry::Passkey => (
                gettext("Enter the passkey"),
                gettext("Type the passkey shown on {device}.").replace("{device}", &capped(name)),
            ),
            Entry::Confirm => (
                gettext("Confirm the code"),
                gettext("Is this the code shown on {device}?\n\n{code}")
                    .replace("{device}", &capped(name))
                    .replace("{code}", code),
            ),
            Entry::Authorize => (
                gettext("Allow pairing"),
                gettext("{device} wants to pair with this computer.")
                    .replace("{device}", &capped(name)),
            ),
            Entry::ShowPin => (
                gettext("Type this PIN"),
                gettext("Type this PIN on {device}. It will not ask again.\n\n{code}")
                    .replace("{device}", &capped(name))
                    .replace("{code}", code),
            ),
            Entry::ShowPasskey => (
                gettext("Type this passkey"),
                gettext("Type this passkey on {device}.\n\n{code}")
                    .replace("{device}", &capped(name))
                    .replace("{code}", code),
            ),
        };
        if self.heading().as_deref() != Some(heading.as_str()) {
            self.set_heading(Some(&heading));
        }
        if self.body() != body {
            self.set_body(&body);
        }
        self.imp().entry.set_placeholder_text(Some(&match kind {
            Entry::Pin => gettext("1 to 16 letters or digits"),
            Entry::Passkey => gettext("Up to 999999"),
            _ => String::new(),
        }));
        self.imp().show_entry(kind);
    }

    pub fn connect_answered<F>(&self, handler: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, PairingAnswer) + 'static,
    {
        self.connect_closure(
            "answered",
            false,
            glib::closure_local!(move |dialog: &Self,
                                       response: String,
                                       value: String,
                                       numeric: bool| {
                handler(dialog, answer(&response, &value, numeric));
            }),
        )
    }
}

fn answer(response: &str, value: &str, numeric: bool) -> PairingAnswer {
    match (response, numeric) {
        (ACCEPT, _) => PairingAnswer::Accept,
        (OK, true) => match value.parse::<u32>() {
            Ok(passkey) if passkey <= PASSKEY_MAX => PairingAnswer::Passkey(passkey),
            _ => PairingAnswer::Deny,
        },
        (OK, false) => PairingAnswer::Pin(value.chars().take(PIN_MAX).collect()),
        _ => PairingAnswer::Deny,
    }
}

fn capped(text: &str) -> String {
    crate::truncate(text, NAME_MAX)
}

#[cfg(test)]
mod tests;
