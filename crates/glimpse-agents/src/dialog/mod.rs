mod imp;

use std::rc::Rc;

use adw::prelude::*;
use gtk4::glib;
use gtk4::glib::subclass::prelude::*;

pub const TITLE_MAX: usize = 120;
pub const BODY_MAX: usize = 240;
pub const DETAIL_MAX: usize = 2000;
pub const ACCOUNT_MAX: usize = 64;
pub const PIN_MAX: usize = 16;
pub const PASSKEY_MAX: u32 = 999_999;
pub const WIFI_KEY_MIN: usize = 8;
pub const WIFI_KEY_MAX: usize = 63;
pub const WIFI_KEY_HEX: usize = 64;

fn description(mut body: &str, tint: &str) -> String {
    let mut markup = String::new();
    let tint = glib::markup_escape_text(tint);
    while let Some((plain, quoted)) = body.split_once('`') {
        let Some((command, rest)) = quoted.split_once('\'') else {
            break;
        };
        markup.push_str(&glib::markup_escape_text(plain));
        markup.push_str(&format!(
            "<span font_family=\"monospace\" foreground=\"{tint}\">{}</span>",
            glib::markup_escape_text(command)
        ));
        body = rest;
    }
    markup.push_str(&glib::markup_escape_text(body));
    markup
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Lock,
    Wifi,
    Vpn,
    Bluetooth,
    Reboot,
    PowerOff,
}

impl Icon {
    fn name(self) -> &'static str {
        match self {
            Self::Lock => "system-lock-screen-symbolic",
            Self::Wifi => "network-wireless-symbolic",
            Self::Vpn => "network-vpn-symbolic",
            Self::Bluetooth => "bluetooth-active-symbolic",
            Self::Reboot => "system-reboot-symbolic",
            Self::PowerOff => "system-shutdown-symbolic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Password,
    Visible,
    WifiKey,
    Pin,
    Passkey,
}

impl Field {
    pub fn accepts(&self, text: &str) -> bool {
        let characters = text.chars().count();
        match self {
            Self::Password | Self::Visible => !text.is_empty(),
            Self::WifiKey => {
                (WIFI_KEY_MIN..=WIFI_KEY_MAX).contains(&characters)
                    || (characters == WIFI_KEY_HEX && text.chars().all(|c| c.is_ascii_hexdigit()))
            }
            Self::Pin => {
                (1..=PIN_MAX).contains(&characters)
                    && text.chars().all(|c| c.is_ascii_alphanumeric())
            }
            Self::Passkey => {
                !text.is_empty()
                    && text.chars().all(|c| c.is_ascii_digit())
                    && text
                        .parse::<u32>()
                        .is_ok_and(|passkey| passkey <= PASSKEY_MAX)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    Info(String),
    Error(String),
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub icon: Icon,
    pub title: String,
    pub body: String,
    pub accounts: Vec<String>,
    pub username: Option<String>,
    pub secret: Option<(String, Field)>,
    pub autoconnect: Option<bool>,
    pub code: Option<String>,
    pub details: Vec<(String, String)>,
    pub note: Option<Note>,
    pub busy: bool,
    pub confirm: Option<String>,
    pub cancel: String,
}

pub enum Response {
    Confirm {
        account: usize,
        username: String,
        secret: crate::ask::Secret,
        autoconnect: bool,
    },
    Cancel,
    Account(usize),
}

pub fn selected_account(index: u32) -> Option<Response> {
    (index != gtk4::INVALID_LIST_POSITION).then_some(Response::Account(index as usize))
}

glib::wrapper! {
    pub struct AgentDialog(ObjectSubclass<imp::AgentDialog>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for AgentDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentDialog {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn show(&self, view: &View) {
        self.imp().show(view);
    }

    pub fn connect_response<F>(&self, handler: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, Response) + 'static,
    {
        let handler = Rc::new(handler);
        let selected = Rc::clone(&handler);
        self.imp().account.connect_selected_notify(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |row| {
                if dialog.imp().account_hold.get() {
                    return;
                }
                if let Some(response) = selected_account(row.selected()) {
                    selected(&dialog, response);
                }
            }
        ));
        let handler = Rc::clone(&handler);
        self.connect_closure(
            "response",
            false,
            glib::closure_local!(move |dialog: &Self, confirmed: bool| {
                let response = match confirmed {
                    true => dialog.imp().confirmation(),
                    false => Response::Cancel,
                };
                handler(dialog, response);
            }),
        )
    }
}

pub fn register_resources() -> Result<(), glib::Error> {
    gio::resources_register_include!("glimpse-agents.gresource")
}

#[cfg(test)]
mod tests;
