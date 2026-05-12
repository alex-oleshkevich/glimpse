use adw::prelude::*;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};

use crate::theme;

const RESPONSE_OK: &str = "ok";

pub struct WarningDialogInit {
    pub parent: gtk::Widget,
}

pub struct WarningDialog {
    parent: gtk::Widget,
    dialog: adw::AlertDialog,
}

#[derive(Debug, Clone)]
pub enum WarningDialogInput {
    Show { heading: String, body: String },
}

#[relm4::component(pub)]
impl SimpleComponent for WarningDialog {
    type Init = WarningDialogInit;
    type Input = WarningDialogInput;
    type Output = ();

    view! {
        gtk::Box {
            set_visible: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let dialog = adw::AlertDialog::new(None, None);
        dialog.add_response(RESPONSE_OK, "OK");
        dialog.set_default_response(Some(RESPONSE_OK));
        dialog.set_close_response(RESPONSE_OK);
        theme::apply_theme_mode(&dialog, &theme::DIALOG_THEME_MODE);

        let model = WarningDialog {
            parent: init.parent,
            dialog,
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WarningDialogInput::Show { heading, body } => {
                self.dialog.set_heading(Some(&heading));
                self.dialog.set_body(&body);
                self.dialog.present(Some(&self.parent));
            }
        }
    }
}
