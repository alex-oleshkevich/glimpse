use std::ffi::{CStr, CString};
use std::time::Duration;

use gettextrs::gettext;
use pam_client2::{Context, ConversationHandler, ErrorCode, Flag};
use zeroize::Zeroizing;

pub const TIMEOUT: Duration = Duration::from_secs(30);
const PAM_TEXT_MAX_CHARS: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    WrongPassword,
    Pam(String),
    TooManyAttempts,
    CantVerify,
    AccountRefused,
    PasswordChange,
    TimedOut,
    Failed,
    ConsoleOnly,
}

impl Message {
    pub fn text(&self) -> String {
        match self {
            Self::WrongPassword => gettext("Wrong password"),
            Self::Pam(text) => text.clone(),
            Self::TooManyAttempts => gettext("Too many attempts"),
            Self::CantVerify => gettext("Can't verify passwords. Run glimpse-lock check."),
            Self::AccountRefused => gettext("This account can't be unlocked here"),
            Self::PasswordChange => gettext("Password change required"),
            Self::TimedOut => gettext("Authentication timed out"),
            Self::Failed => gettext("Authentication failed"),
            Self::ConsoleOnly => gettext(
                "Can't verify passwords. Switch to a text console (Ctrl+Alt+F2), log in, and run glimpse-lock check.",
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Unlock { password_expired: bool },
    Refused { message: Message, shake: bool },
}

pub struct Service(String);

impl Service {
    pub fn new(configured: &str) -> Self {
        Self(configured.to_ascii_lowercase())
    }

    pub fn name(&self) -> &str {
        &self.0
    }

    pub fn reload(&mut self, configured: &str) {
        if configured.to_ascii_lowercase() != self.0 {
            tracing::warn!(
                running = %self.0,
                configured,
                "[lock] pam-service changes apply after a restart"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Authenticate,
    Account,
}

pub fn classify(result: Result<(), (Step, ErrorCode)>, text: Option<&str>) -> Verdict {
    let pam_or = |fallback: Message| text.map_or(fallback, |text| Message::Pam(text.to_owned()));
    let code = match result {
        Ok(()) => {
            return Verdict::Unlock {
                password_expired: false,
            };
        }
        Err((Step::Account, ErrorCode::NEW_AUTHTOK_REQD | ErrorCode::AUTHTOK_EXPIRED)) => {
            return Verdict::Unlock {
                password_expired: true,
            };
        }
        Err((_, code)) => code,
    };
    let (message, shake) = match code {
        ErrorCode::AUTH_ERR | ErrorCode::USER_UNKNOWN => (pam_or(Message::WrongPassword), true),
        ErrorCode::MAXTRIES => (pam_or(Message::TooManyAttempts), true),
        ErrorCode::AUTHINFO_UNAVAIL => (Message::CantVerify, false),
        ErrorCode::ACCT_EXPIRED | ErrorCode::PERM_DENIED => {
            (pam_or(Message::AccountRefused), false)
        }
        ErrorCode::NEW_AUTHTOK_REQD | ErrorCode::AUTHTOK_EXPIRED => {
            (Message::PasswordChange, false)
        }
        code => {
            tracing::warn!(?code, "authentication failed");
            (Message::Failed, false)
        }
    };
    Verdict::Refused { message, shake }
}

pub struct Conversation {
    user: String,
    password: Option<Zeroizing<String>>,
    text: Option<String>,
}

impl Conversation {
    pub fn new(user: &str, password: Zeroizing<String>) -> Self {
        Self {
            user: user.to_owned(),
            password: Some(password),
            text: None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    fn capture(&mut self, message: &CStr) {
        let text = glimpse_utils::clean(&message.to_string_lossy(), PAM_TEXT_MAX_CHARS);
        if !text.is_empty() {
            self.text = Some(text);
        }
    }
}

impl ConversationHandler for Conversation {
    fn prompt_echo_on(&mut self, _prompt: &CStr) -> Result<CString, ErrorCode> {
        CString::new(self.user.as_str()).map_err(|_| ErrorCode::CONV_ERR)
    }

    fn prompt_echo_off(&mut self, _prompt: &CStr) -> Result<CString, ErrorCode> {
        let password = self.password.take().ok_or(ErrorCode::CONV_ERR)?;
        CString::new(password.as_bytes()).map_err(|_| ErrorCode::CONV_ERR)
    }

    fn text_info(&mut self, message: &CStr) {
        self.capture(message);
    }

    fn error_msg(&mut self, message: &CStr) {
        self.capture(message);
    }
}

pub fn verify(service: &str, user: &str, password: Zeroizing<String>) -> Verdict {
    let mut context = match Context::new(service, Some(user), Conversation::new(user, password)) {
        Ok(context) => context,
        Err(error) => return classify(Err((Step::Authenticate, error.code())), None),
    };
    let result = context
        .authenticate(Flag::NONE)
        .map_err(|error| (Step::Authenticate, error.code()))
        .and_then(|()| {
            context
                .acct_mgmt(Flag::NONE)
                .map_err(|error| (Step::Account, error.code()))
        });
    let verdict = classify(result, context.conversation().text());
    if matches!(verdict, Verdict::Unlock { .. })
        && let Err(error) = context.reinitialize_credentials(Flag::NONE)
    {
        tracing::warn!(code = ?error.code(), "refreshing credentials failed");
    }
    verdict
}

pub fn spawn(
    service: String,
    user: String,
    password: Zeroizing<String>,
) -> std::io::Result<tokio::sync::oneshot::Receiver<Verdict>> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("pam".to_owned())
        .spawn(move || {
            let _ = sender.send(verify(&service, &user, password));
        })?;
    Ok(receiver)
}

#[cfg(test)]
mod tests;
