mod imp;

use adw::prelude::*;
use gettextrs::gettext;
use gtk4::glib;
use gtk4::glib::subclass::prelude::*;

use imp::{ALLOW, CANCEL, CONFIRM, DENY, Entry, NAME_MAX, OK};

pub use imp::{PASSKEY_MAX, PIN_MAX};

glib::wrapper! {
    pub struct PairingDialog(ObjectSubclass<imp::PairingDialog>)
        @extends adw::AlertDialog, adw::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingAnswer {
    Confirm,
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

    pub fn show_confirm(&self, device: &str, passkey: u32) {
        self.imp().present_prompt(
            &gettext("Confirm the pairing code"),
            &gettext("Check that {device} is showing the same six digits.")
                .replace("{device}", &capped(device)),
            &[
                (CANCEL, gettext("Cancel"), adw::ResponseAppearance::Default),
                (
                    CONFIRM,
                    gettext("Confirm"),
                    adw::ResponseAppearance::Suggested,
                ),
            ],
            CANCEL,
        );
        self.imp().show_code(Some(&digits(passkey)));
        self.imp().show_progress(None);
        self.imp().show_entry(Entry::None);
    }

    pub fn show_authorize(&self, device: &str) {
        self.imp().present_prompt(
            &gettext("Allow this device to pair?"),
            &gettext("{device} is asking to pair with this computer.")
                .replace("{device}", &capped(device)),
            &[
                (DENY, gettext("Deny"), adw::ResponseAppearance::Default),
                (ALLOW, gettext("Allow"), adw::ResponseAppearance::Suggested),
            ],
            DENY,
        );
        self.imp().show_code(None);
        self.imp().show_progress(None);
        self.imp().show_entry(Entry::None);
    }

    pub fn show_request_pin(&self, device: &str) {
        self.imp().present_prompt(
            &gettext("Enter the PIN"),
            &gettext("Type the PIN shown on {device}, then press OK.")
                .replace("{device}", &capped(device)),
            &[
                (CANCEL, gettext("Cancel"), adw::ResponseAppearance::Default),
                (OK, gettext("OK"), adw::ResponseAppearance::Suggested),
            ],
            CANCEL,
        );
        self.set_default_response(Some(OK));
        self.imp().show_code(None);
        self.imp().show_progress(None);
        self.imp()
            .entry
            .set_placeholder_text(Some(&gettext("1 to 16 letters or digits")));
        self.imp().show_entry(Entry::Pin);
    }

    pub fn show_request_passkey(&self, device: &str) {
        self.imp().present_prompt(
            &gettext("Enter the passkey"),
            &gettext("Type the passkey shown on {device}.").replace("{device}", &capped(device)),
            &[
                (CANCEL, gettext("Cancel"), adw::ResponseAppearance::Default),
                (OK, gettext("OK"), adw::ResponseAppearance::Suggested),
            ],
            CANCEL,
        );
        self.set_default_response(Some(OK));
        self.imp().show_code(None);
        self.imp().show_progress(None);
        self.imp()
            .entry
            .set_placeholder_text(Some(&gettext("Up to 999999")));
        self.imp().show_entry(Entry::Passkey);
    }

    pub fn show_display_pin(&self, device: &str, pin: &str) {
        self.imp().present_prompt(
            &gettext("Type this PIN on the device"),
            &gettext("{device} will not ask again, so enter it now.")
                .replace("{device}", &capped(device)),
            &[(CANCEL, gettext("Cancel"), adw::ResponseAppearance::Default)],
            CANCEL,
        );
        self.imp().show_code(Some(&capped(pin)));
        self.imp().show_progress(None);
        self.imp().show_entry(Entry::None);
    }

    pub fn show_display_passkey(&self, device: &str, passkey: u32, entered: u16) {
        self.imp().present_prompt(
            &gettext("Type this passkey on the device"),
            &gettext("{device} is waiting. It will pair once the last digit is entered.")
                .replace("{device}", &capped(device)),
            &[(CANCEL, gettext("Cancel"), adw::ResponseAppearance::Default)],
            CANCEL,
        );
        self.imp().show_code(Some(&digits(passkey)));
        self.imp().show_progress(Some(
            &gettext("{entered} of 6 entered").replace("{entered}", &entered.min(6).to_string()),
        ));
        self.imp().show_entry(Entry::None);
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
        (CONFIRM | ALLOW, _) => PairingAnswer::Confirm,
        (OK, true) => match value.parse::<u32>() {
            Ok(passkey) if passkey <= PASSKEY_MAX => PairingAnswer::Passkey(passkey),
            _ => PairingAnswer::Deny,
        },
        (OK, false) => PairingAnswer::Pin(value.chars().take(PIN_MAX).collect()),
        _ => PairingAnswer::Deny,
    }
}

fn digits(passkey: u32) -> String {
    let padded = format!("{:06}", passkey.min(PASSKEY_MAX));
    format!("{} {}", &padded[..3], &padded[3..])
}

fn capped(text: &str) -> String {
    crate::truncate(text, NAME_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use imp::{Entry, accepts};

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
    fn a_passkey_is_shown_as_six_zero_padded_digits() {
        assert_eq!(digits(418_209), "418 209");
        assert_eq!(digits(42), "000 042");
        assert_eq!(digits(u32::MAX), "999 999");
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
        assert_eq!(answer(CONFIRM, "", false), PairingAnswer::Confirm);
        assert_eq!(answer(ALLOW, "", false), PairingAnswer::Confirm);
        assert_eq!(answer(CANCEL, "", false), PairingAnswer::Deny);
        assert_eq!(answer(DENY, "", false), PairingAnswer::Deny);
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
