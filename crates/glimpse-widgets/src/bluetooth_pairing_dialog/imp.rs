use adw::{prelude::*, subclass::prelude::*};
use gtk4::{CompositeTemplate, TemplateChild, glib, glib::subclass::Signal};
use std::cell::Cell;
use std::sync::OnceLock;

use crate::{set_css_class, set_text_capped};

pub const CANCEL: &str = "cancel";
pub const CONFIRM: &str = "confirm";
pub const DENY: &str = "deny";
pub const ALLOW: &str = "allow";
pub const OK: &str = "ok";

const RESPONSES: [&str; 5] = [CANCEL, CONFIRM, DENY, ALLOW, OK];

pub const PIN_MAX: usize = 16;
pub const PASSKEY_MAX: u32 = 999_999;
pub const NAME_MAX: usize = 48;
pub const CODE_MAX: usize = 32;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    #[default]
    None,
    Pin,
    Passkey,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/bluetooth_pairing_dialog.ui")]
pub struct PairingDialog {
    #[template_child]
    pub code: TemplateChild<gtk4::Label>,
    #[template_child]
    pub progress: TemplateChild<gtk4::Label>,
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

        let dialog = self.obj().clone();
        self.obj().connect_response(None, move |_, response| {
            let kind = dialog.imp().entry_kind.get();
            let value = match kind {
                Entry::None => String::new(),
                _ => dialog.imp().entry.text().to_string(),
            };
            dialog.emit_by_name::<()>(
                "answered",
                &[&response.to_owned(), &value, &(kind == Entry::Passkey)],
            );
        });
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for PairingDialog {}
impl AdwDialogImpl for PairingDialog {}
impl AdwAlertDialogImpl for PairingDialog {}

impl PairingDialog {
    pub fn present_prompt(
        &self,
        heading: &str,
        body: &str,
        responses: &[(&str, String, adw::ResponseAppearance)],
        close: &str,
    ) {
        let dialog = self.obj();
        if dialog.heading().as_deref() != Some(heading) {
            dialog.set_heading(Some(heading));
        }
        if dialog.body() != body {
            dialog.set_body(body);
        }

        for response in RESPONSES {
            if dialog.has_response(response) {
                dialog.remove_response(response);
            }
        }
        for (id, label, appearance) in responses {
            dialog.add_response(id, label);
            dialog.set_response_appearance(id, *appearance);
        }
        dialog.set_close_response(close);
    }

    pub fn show_code(&self, code: Option<&str>) {
        set_text_capped(&self.code, code, CODE_MAX);
    }

    pub fn show_progress(&self, text: Option<&str>) {
        set_text_capped(&self.progress, text, CODE_MAX);
    }

    pub fn show_entry(&self, kind: Entry) {
        self.entry_kind.set(kind);
        let visible = kind != Entry::None;
        if self.entry.get_visible() != visible {
            self.entry.set_visible(visible);
        }
        if !visible {
            return;
        }

        self.entry.set_text("");
        match kind {
            Entry::None => return,
            Entry::Pin => {
                self.entry.set_max_length(PIN_MAX as i32);
                self.entry.set_input_purpose(gtk4::InputPurpose::FreeForm);
            }
            Entry::Passkey => {
                self.entry.set_max_length(6);
                self.entry.set_input_purpose(gtk4::InputPurpose::Digits);
            }
        }
        self.revalidate();
    }

    pub fn revalidate(&self) {
        let kind = self.entry_kind.get();
        if kind == Entry::None {
            return;
        }
        let text = self.entry.text();
        let valid = accepts(kind, &text);
        self.obj().set_response_enabled(OK, valid);
        set_css_class(&*self.entry, "error", !valid && !text.is_empty());
    }
}

pub fn accepts(kind: Entry, text: &str) -> bool {
    match kind {
        Entry::None => true,
        Entry::Pin => {
            let length = text.chars().count();
            (1..=PIN_MAX).contains(&length) && text.chars().all(|c| c.is_ascii_alphanumeric())
        }
        Entry::Passkey => text
            .parse::<u32>()
            .is_ok_and(|passkey| passkey <= PASSKEY_MAX),
    }
}
