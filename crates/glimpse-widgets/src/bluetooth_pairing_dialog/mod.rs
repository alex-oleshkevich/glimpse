mod imp;

use adw::prelude::*;
use gettextrs::gettext;
use gtk4::glib;
use gtk4::glib::subclass::prelude::*;

use imp::{NAME_MAX, OK};

pub use imp::{Entry, PASSKEY_MAX, PIN_MAX};

glib::wrapper! {
    pub struct PairingDialog(ObjectSubclass<imp::PairingDialog>)
        @extends adw::AlertDialog, adw::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingAnswer {
    Deny,
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

    pub fn ask(&self, device: &str, kind: Entry) {
        let (heading, body) = match kind {
            Entry::Pin => (
                gettext("Enter the PIN"),
                gettext("Type the PIN shown on {device}, then press OK.")
                    .replace("{device}", &capped(device)),
            ),
            Entry::Passkey => (
                gettext("Enter the passkey"),
                gettext("Type the passkey shown on {device}.").replace("{device}", &capped(device)),
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
mod tests {
    use super::*;
    use imp::{CANCEL, accepts};

    #[test]
    fn a_pin_is_one_to_sixteen_alphanumeric_characters() {
        assert!(accepts(Entry::Pin, "0000"));
        assert!(accepts(Entry::Pin, &"a".repeat(PIN_MAX)));
        assert!(!accepts(Entry::Pin, ""));
        assert!(!accepts(Entry::Pin, &"a".repeat(PIN_MAX + 1)));
        assert!(!accepts(Entry::Pin, "12 34"));
        assert!(!accepts(Entry::Pin, "пин"));
    }

    #[test]
    fn a_passkey_stops_at_six_nines() {
        assert!(accepts(Entry::Passkey, "0"));
        assert!(accepts(Entry::Passkey, "999999"));
        assert!(!accepts(Entry::Passkey, "1000000"));
        assert!(!accepts(Entry::Passkey, ""));
        assert!(!accepts(Entry::Passkey, "-1"));
        assert!(!accepts(Entry::Passkey, "12a"));
    }

    #[test]
    fn a_multibyte_name_is_capped_by_characters() {
        let name = "Наушники ".repeat(20);

        let capped = capped(&name);

        assert_eq!(capped.chars().count(), NAME_MAX);
        assert!(name.starts_with(&capped));
    }

    #[test]
    fn a_typed_answer_is_read_back_from_the_response_and_its_value() {
        assert_eq!(answer(CANCEL, "", false), PairingAnswer::Deny);
        assert_eq!(answer(OK, "123456", true), PairingAnswer::Passkey(123_456));
        assert_eq!(
            answer(OK, "0000", false),
            PairingAnswer::Pin("0000".to_owned()),
            "a numeric PIN is a PIN; only the prompt knows which was asked for"
        );
        assert_eq!(
            answer(OK, "abcd", true),
            PairingAnswer::Deny,
            "the entry cannot enable OK on this, so it can only mean the value went missing"
        );
    }
}
