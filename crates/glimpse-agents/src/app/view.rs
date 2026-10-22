use gettextrs::gettext;

use crate::ask::Prompt;
use crate::bluetooth::{self, Kind};
use crate::dialog::{Field, Icon, Note, View};
use crate::network;
use crate::polkit::{self, Conversation};

const NAME_MAX: usize = 64;
const CODE_MAX: usize = 16;
const USERNAME_HINT: &str = "username";

pub fn view_for(prompt: &Prompt, conversation: Option<&Conversation>) -> View {
    match prompt {
        Prompt::Polkit(request) => authentication(request, conversation),
        Prompt::Secret(request) if request.setting == "vpn" => vpn(request),
        Prompt::Secret(request) => wifi(request),
        Prompt::Pairing(request) => pairing(request),
    }
}

pub fn asks_username(request: &network::Request) -> bool {
    request.setting == "vpn" && request.hints.iter().any(|hint| hint == USERNAME_HINT)
}

fn blank(icon: Icon, title: String, body: String) -> View {
    View {
        icon,
        title,
        body,
        accounts: Vec::new(),
        username: None,
        secret: None,
        autoconnect: None,
        code: None,
        details: Vec::new(),
        note: None,
        busy: false,
        confirm: None,
        cancel: gettext("Cancel"),
    }
}

fn authentication(request: &polkit::Request, conversation: Option<&Conversation>) -> View {
    let (label, field) = match conversation.and_then(|conversation| conversation.prompt.as_ref()) {
        Some((text, echo)) => (
            text.trim().trim_end_matches(':').trim().to_owned(),
            if *echo {
                Field::Visible
            } else {
                Field::Password
            },
        ),
        None => (String::new(), Field::Password),
    };
    let label = match label.is_empty() {
        true => gettext("Password"),
        false => label,
    };
    let mut details = request.details.clone();
    details.push((gettext("Action"), request.action_id.clone()));
    View {
        accounts: request
            .identities
            .iter()
            .map(|identity| match identity.name.is_empty() {
                true => identity.user.clone(),
                false => identity.name.clone(),
            })
            .collect(),
        secret: Some((label, field)),
        details,
        note: conversation.and_then(|conversation| conversation.note.clone()),
        busy: conversation.is_some_and(|conversation| conversation.busy),
        confirm: Some(gettext("Authorize")),
        ..blank(
            Icon::Lock,
            gettext("Authentication needed"),
            request.message.clone(),
        )
    }
}

fn wifi(request: &network::Request) -> View {
    View {
        secret: Some((gettext("Password"), Field::WifiKey)),
        autoconnect: Some(request.autoconnect),
        note: request
            .retry
            .then(|| Note::Error(gettext("That password didn't work"))),
        confirm: Some(match request.retry {
            true => gettext("Try again"),
            false => gettext("Join"),
        }),
        ..blank(
            Icon::Wifi,
            gettext("Join “{name}”").replace("{name}", &name(&request.name)),
            gettext("The network needs a password"),
        )
    }
}

fn vpn(request: &network::Request) -> View {
    View {
        username: asks_username(request).then(String::new),
        secret: Some((gettext("Password"), Field::Password)),
        confirm: Some(gettext("Connect")),
        ..blank(
            Icon::Vpn,
            gettext("Sign in to “{name}”").replace("{name}", &name(&request.name)),
            String::new(),
        )
    }
}

fn pairing(request: &bluetooth::Request) -> View {
    let name = match request.name.trim().is_empty() {
        true => gettext("this device"),
        false => name(&request.name),
    };
    let pair = || gettext("Pair with “{name}”").replace("{name}", &name);
    let view = |title: String, body: String| blank(Icon::Bluetooth, title, body);
    match &request.kind {
        Kind::Confirm { passkey } => View {
            code: Some(passkey_code(*passkey)),
            confirm: Some(gettext("Pair")),
            ..view(
                gettext("Pair with “{name}”?").replace("{name}", &name),
                gettext("Check that the device shows the same code"),
            )
        },
        Kind::Pin => View {
            secret: Some((gettext("PIN"), Field::Pin)),
            confirm: Some(gettext("Pair")),
            ..view(
                pair(),
                gettext("Enter the PIN from the device's manual — often 0000"),
            )
        },
        Kind::Passkey => View {
            secret: Some((gettext("Code"), Field::Passkey)),
            confirm: Some(gettext("Pair")),
            ..view(pair(), gettext("Enter the code shown on the device"))
        },
        Kind::ShowPin { pin } => View {
            code: Some(glimpse_utils::clean(pin, CODE_MAX)),
            ..view(
                pair(),
                gettext("Type this code on the device, then press Enter"),
            )
        },
        Kind::ShowPasskey { passkey, .. } => View {
            code: Some(passkey_code(*passkey)),
            ..view(
                pair(),
                gettext("Type this code on the device, then press Enter"),
            )
        },
        Kind::Authorize => View {
            confirm: Some(gettext("Allow")),
            cancel: gettext("Deny"),
            ..view(
                gettext("“{name}” wants to connect").replace("{name}", &name),
                gettext("It has never been paired with this computer"),
            )
        },
    }
}

fn name(name: &str) -> String {
    glimpse_utils::clean(name, NAME_MAX)
}

fn passkey_code(passkey: u32) -> String {
    let digits = format!("{passkey:06}");
    let (head, tail) = digits.split_at(digits.len() - 3);
    format!("{head} {tail}")
}
