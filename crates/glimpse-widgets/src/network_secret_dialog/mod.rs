mod imp;

use adw::prelude::*;
use gettextrs::gettext;
use gtk4::glib;
use gtk4::glib::subclass::prelude::*;

use imp::{CONNECT, NAME_MAX};

glib::wrapper! {
    pub struct SecretDialog(ObjectSubclass<imp::SecretDialog>)
        @extends adw::AlertDialog, adw::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretAnswer {
    Refused,
    Secret(String),
}

impl Default for SecretDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretDialog {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn ask(&self, key: &str, network: &str, retry: bool) {
        if self.imp().asked.replace(key.to_owned()) != key || retry {
            self.imp().entry.set_text("");
        }

        let name = capped(network);
        let (heading, body) = match retry {
            true => (
                gettext("That password did not work"),
                gettext("{network} refused it. Check it and try again.")
                    .replace("{network}", &name),
            ),
            false => (
                gettext("Enter the password for {network}").replace("{network}", &name),
                gettext("The network needs a password before this computer can join it."),
            ),
        };

        if self.heading().as_deref() != Some(heading.as_str()) {
            self.set_heading(Some(&heading));
        }
        if self.body() != body {
            self.set_body(&body);
        }
        self.set_response_label(
            CONNECT,
            &match retry {
                true => gettext("Try again"),
                false => gettext("Connect"),
            },
        );
        self.imp().revalidate();
    }

    #[cfg(test)]
    pub(crate) fn type_in(&self, text: &str) {
        self.imp().entry.set_text(text);
    }

    #[cfg(test)]
    pub(crate) fn is_blank(&self) -> bool {
        self.imp().entry.text().is_empty()
    }

    pub fn connect_answered<F>(&self, handler: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, SecretAnswer) + 'static,
    {
        self.connect_closure(
            "answered",
            false,
            glib::closure_local!(move |dialog: &Self, response: String, secret: String| {
                handler(dialog, answer(&response, &secret));
            }),
        )
    }
}

fn answer(response: &str, secret: &str) -> SecretAnswer {
    match response == CONNECT && imp::accepts(secret) {
        true => SecretAnswer::Secret(secret.to_owned()),
        false => SecretAnswer::Refused,
    }
}

fn capped(text: &str) -> String {
    crate::truncate(text, NAME_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use imp::{CANCEL, SECRET_MAX, accepts};

    #[test]
    fn an_empty_box_cannot_be_submitted_but_a_short_one_can() {
        assert!(!accepts(""));
        assert!(
            accepts("abc"),
            "a WEP key or a short passphrase is still a real secret"
        );
        assert!(accepts(&"a".repeat(SECRET_MAX)));
        assert!(!accepts(&"a".repeat(SECRET_MAX + 1)));
    }

    #[test]
    fn cancelling_is_a_refusal_and_never_an_empty_password() {
        assert_eq!(answer(CANCEL, ""), SecretAnswer::Refused);
        assert_eq!(
            answer(CANCEL, "hunter2hunter2"),
            SecretAnswer::Refused,
            "a typed secret is discarded when the user says no"
        );
    }

    #[test]
    fn connecting_returns_what_was_typed() {
        assert_eq!(
            answer(CONNECT, "hunter2hunter2"),
            SecretAnswer::Secret("hunter2hunter2".to_owned())
        );
        assert_eq!(
            answer(CONNECT, ""),
            SecretAnswer::Refused,
            "an empty secret must never reach NetworkManager"
        );
    }

    #[test]
    fn a_hostile_network_name_is_capped_by_characters() {
        let name = "Kaffeehaus Freies WLAN Gäste-Zugang ".repeat(6);
        let capped = capped(&name);

        assert_eq!(capped.chars().count(), NAME_MAX);
        assert!(name.starts_with(&capped));
    }
}
