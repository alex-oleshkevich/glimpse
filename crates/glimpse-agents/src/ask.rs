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
    Dialog {
        source: Source,
        key: String,
        response: crate::dialog::Response,
    },
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
        }
    }
}
