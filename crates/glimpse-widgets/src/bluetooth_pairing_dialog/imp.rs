use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk4::{CompositeTemplate, TemplateChild, glib, glib::subclass::Signal};
use std::cell::Cell;
use std::sync::OnceLock;

use crate::set_css_class;

pub const CANCEL: &str = "cancel";
pub const OK: &str = "ok";

pub const PIN_MAX: usize = 16;
pub const PASSKEY_DIGITS: i32 = 6;
pub const PASSKEY_MAX: u32 = 999_999;
pub const NAME_MAX: usize = 48;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    #[default]
    Pin,
    Passkey,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/bluetooth_pairing_dialog.ui")]
pub struct PairingDialog {
    #[template_child]
    pub entry: TemplateChild<gtk4::Entry>,

    pub entry_kind: Cell<Entry>,
}

#[glib::object_subclass]
impl ObjectSubclass for PairingDialog {
    const NAME: &'static str = "PairingDialog";
    type Type = super::PairingDialog;
    type ParentType = adw::AlertDialog;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for PairingDialog {
    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("answered")
                    .param_types([
                        String::static_type(),
                        String::static_type(),
                        bool::static_type(),
                    ])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let dialog = self.obj().clone();
        self.entry.connect_changed(move |_| {
            dialog.imp().revalidate();
        });

        let entry = self.entry.get();
        self.obj().connect_map(move |_| {
            entry.grab_focus();
        });

        let dialog = self.obj().clone();
        self.obj().connect_response(None, move |_, response| {
            let kind = dialog.imp().entry_kind.get();
            let value = dialog.imp().entry.text().to_string();
            dialog.emit_by_name::<()>(
                "answered",
                &[&response.to_owned(), &value, &(kind == Entry::Passkey)],
            );
        });

        let dialog = self.obj();
        dialog.add_response(CANCEL, &gettext("Cancel"));
        dialog.set_response_appearance(CANCEL, adw::ResponseAppearance::Default);
        dialog.add_response(OK, &gettext("OK"));
        dialog.set_response_appearance(OK, adw::ResponseAppearance::Suggested);
        dialog.set_close_response(CANCEL);
        dialog.set_default_response(Some(OK));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for PairingDialog {}
impl AdwDialogImpl for PairingDialog {}
impl AdwAlertDialogImpl for PairingDialog {}

impl PairingDialog {
    pub fn show_entry(&self, kind: Entry) {
        if self.entry_kind.replace(kind) != kind {
            self.entry.set_text("");
        }
        match kind {
            Entry::Pin => {
                self.entry.set_max_length(PIN_MAX as i32);
                self.entry.set_input_purpose(gtk4::InputPurpose::FreeForm);
            }
            Entry::Passkey => {
                self.entry.set_max_length(PASSKEY_DIGITS);
                self.entry.set_input_purpose(gtk4::InputPurpose::Digits);
            }
        }
        self.revalidate();
    }

    pub fn revalidate(&self) {
        let kind = self.entry_kind.get();
        let text = self.entry.text();
        let valid = accepts(kind, &text);
        self.obj().set_response_enabled(OK, valid);
        set_css_class(&*self.entry, "error", !valid && !text.is_empty());
    }
}

pub fn accepts(kind: Entry, text: &str) -> bool {
    match kind {
        Entry::Pin => {
            let length = text.chars().count();
            (1..=PIN_MAX).contains(&length) && text.chars().all(|c| c.is_ascii_alphanumeric())
        }
        Entry::Passkey => text
            .parse::<u32>()
            .is_ok_and(|passkey| passkey <= PASSKEY_MAX),
    }
}
