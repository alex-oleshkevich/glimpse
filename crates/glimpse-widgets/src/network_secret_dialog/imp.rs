use adw::{prelude::*, subclass::prelude::*};

use crate::network_popover::{Entered, accepts};
use gettextrs::gettext;
use gtk4::{CompositeTemplate, TemplateChild, glib, glib::subclass::Signal};
use std::cell::RefCell;
use std::sync::OnceLock;

pub const CANCEL: &str = "cancel";
pub const CONNECT: &str = "connect";

pub const NAME_MAX: usize = 48;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/network_secret_dialog.ui")]
pub struct SecretDialog {
    #[template_child]
    pub entry: TemplateChild<gtk4::PasswordEntry>,

    pub asked: RefCell<String>,
    pub entered: std::cell::Cell<Entered>,
}

#[glib::object_subclass]
impl ObjectSubclass for SecretDialog {
    const NAME: &'static str = "SecretDialog";
    type Type = super::SecretDialog;
    type ParentType = adw::AlertDialog;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for SecretDialog {
    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("answered")
                    .param_types([String::static_type(), String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let object = self.obj();

        object.add_response(CANCEL, &gettext("Cancel"));
        object.add_response(CONNECT, &gettext("Connect"));
        object.set_response_appearance(CONNECT, adw::ResponseAppearance::Suggested);
        object.set_default_response(Some(CONNECT));
        object.set_close_response(CANCEL);

        self.entry.connect_changed(glib::clone!(
            #[weak]
            object,
            move |_| object.imp().revalidate()
        ));

        object.connect_response(None, move |dialog, response| {
            let secret = match response == CONNECT {
                true => dialog.imp().entry.text().to_string(),
                false => String::new(),
            };
            dialog.emit_by_name::<()>("answered", &[&response.to_owned(), &secret]);
        });

        self.revalidate();
    }
}

impl SecretDialog {
    pub fn revalidate(&self) {
        self.obj()
            .set_response_enabled(CONNECT, accepts(&self.entry.text(), self.entered.get()));
    }
}

impl WidgetImpl for SecretDialog {}
impl AdwDialogImpl for SecretDialog {}
impl AdwAlertDialogImpl for SecretDialog {}
