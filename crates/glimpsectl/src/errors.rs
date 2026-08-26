use std::process::ExitCode;

use glimpse_ipc::{CallError, ConnectError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Unreachable = 3,
    Unknown = 4,
    Timeout = 5,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

/// `downcast_ref` sees through every `.context(…)` layer, which is what stops a message added
/// upstream from changing the code a script reads.
pub fn exit(error: &anyhow::Error) -> Exit {
    if let Some(connect) = error.downcast_ref::<ConnectError>() {
        return match connect {
            ConnectError::NotListening { .. } | ConnectError::Handshake => Exit::Unreachable,
        };
    }

    if let Some(call) = error.downcast_ref::<CallError>() {
        return match call.code {
            ErrorCode::UnknownTopic | ErrorCode::UnknownCommand => Exit::Unknown,
            ErrorCode::Timeout => Exit::Timeout,
            _ => Exit::Failed,
        };
    }

    Exit::Failed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_does_not_change_the_code_a_script_sees() {
        let error = anyhow::Error::new(CallError::new(ErrorCode::UnknownTopic, "nope"))
            .context("while reading battery.status");
        assert_eq!(exit(&error), Exit::Unknown);
    }

    #[test]
    fn a_timeout_is_five_and_anything_else_is_one() {
        let timed_out = anyhow::Error::new(CallError::new(ErrorCode::Timeout, "no answer"));
        assert_eq!(exit(&timed_out), Exit::Timeout);
        assert_eq!(exit(&anyhow::anyhow!("something else")), Exit::Failed);
    }
}
