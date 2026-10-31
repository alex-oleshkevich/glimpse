use std::fmt;

use tokio::sync::oneshot;
use zeroize::Zeroizing;

pub type Secret = Zeroizing<String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Polkit,
    Network,
    Bluetooth,
}

pub struct Ask {
    pub source: Source,
    pub key: String,
    pub prompt: Prompt,
    pub reply: Reply,
}

pub enum Prompt {
    Polkit(crate::polkit::Request),
    Secret(crate::network::Request),
    Pairing(crate::bluetooth::Request),
}

pub enum Reply {
    Polkit(oneshot::Sender<crate::polkit::Outcome>),
    Secret(oneshot::Sender<crate::network::Answer>),
    Pairing(oneshot::Sender<crate::bluetooth::Answer>),
    Shown,
}

pub enum Input {
    Ask(Ask),
    Withdraw {
        source: Source,
        key: Option<String>,
    },
    Line {
        key: String,
        line: crate::polkit::helper::Line,
    },
    Locked(bool),
    LockUnknown,
    Osd(crate::osd::Message),
    OsdHide,
    OsdExpired(u64),
    OsdGone(u64),
    SessionRequest(crate::session::Request),
    SessionReady {
        id: u64,
        view: Result<crate::dialog::View, String>,
    },
    SessionResponse {
        id: u64,
        response: crate::dialog::Response,
    },
    SessionFocus {
        id: u64,
        active: bool,
    },
    SessionFocusTimeout(u64),
    SessionDone(Result<(), String>),
    Dialog {
        source: Source,
        key: String,
        response: crate::dialog::Response,
    },
    ConfigChanged(Box<glimpse_config::Config>),
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ask(ask) => f.debug_tuple("Ask").field(&ask.source).finish(),
            Self::Withdraw { source, key } => f
                .debug_struct("Withdraw")
                .field("source", source)
                .field("all", &key.is_none())
                .finish(),
            Self::Line { .. } => f.write_str("Line"),
            Self::Locked(locked) => f.debug_tuple("Locked").field(locked).finish(),
            Self::LockUnknown => f.write_str("LockUnknown"),
            Self::Osd(_) => f.write_str("Osd"),
            Self::OsdHide => f.write_str("OsdHide"),
            Self::OsdExpired(_) => f.write_str("OsdExpired"),
            Self::OsdGone(_) => f.write_str("OsdGone"),
            Self::SessionRequest(_) => f.write_str("SessionRequest"),
            Self::SessionReady { .. } => f.write_str("SessionReady"),
            Self::SessionResponse { .. } => f.write_str("SessionResponse"),
            Self::SessionFocus { .. } => f.write_str("SessionFocus"),
            Self::SessionFocusTimeout(_) => f.write_str("SessionFocusTimeout"),
            Self::SessionDone(_) => f.write_str("SessionDone"),
            Self::Dialog {
                response: crate::dialog::Response::Confirm { .. },
                ..
            } => f.write_str("Dialog(Confirm)"),
            Self::Dialog {
                response: crate::dialog::Response::Cancel,
                ..
            } => f.write_str("Dialog(Cancel)"),
            Self::Dialog {
                response: crate::dialog::Response::Account(account),
                ..
            } => write!(f, "Dialog(Account({account}))"),
            Self::ConfigChanged(_) => f.write_str("ConfigChanged"),
        }
    }
}
