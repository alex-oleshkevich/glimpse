use std::{
    cell::Cell,
    ffi::{CStr, CString},
    rc::Rc,
};

use pam_client2::{Context, ConversationHandler, ErrorCode, Flag};
use zeroize::{Zeroize, Zeroizing};

pub struct SecretString {
    inner: Zeroizing<String>,
}

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            inner: Zeroizing::new(value.into()),
        }
    }

    fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Clone for SecretString {
    fn clone(&self) -> Self {
        Self::new(self.inner.as_str())
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretString").finish_non_exhaustive()
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.inner.zeroize();
    }
}

pub trait Authenticator: Send + Sync + 'static {
    fn authenticate(
        &self,
        service: &str,
        username: &str,
        password: SecretString,
    ) -> anyhow::Result<AuthResult>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthResult {
    Success,
    Failure {
        pam_message: Option<String>,
    },
    SecondFactorRequired,
    AccountUnavailable {
        reason: String,
        pam_message: Option<String>,
    },
}

#[derive(Debug, Default)]
pub struct PamAuthenticator;

impl Authenticator for PamAuthenticator {
    fn authenticate(
        &self,
        service: &str,
        username: &str,
        password: SecretString,
    ) -> anyhow::Result<AuthResult> {
        authenticate_with_pam(service, username, password)
    }
}

#[derive(Debug)]
pub struct PreviewAuthenticator {
    valid_password: String,
}

impl Default for PreviewAuthenticator {
    fn default() -> Self {
        Self {
            valid_password: "valid".into(),
        }
    }
}

impl PreviewAuthenticator {
    pub fn valid_password(&self) -> &str {
        &self.valid_password
    }
}

impl Authenticator for PreviewAuthenticator {
    fn authenticate(
        &self,
        _service: &str,
        _username: &str,
        password: SecretString,
    ) -> anyhow::Result<AuthResult> {
        if password.as_str() == self.valid_password {
            Ok(AuthResult::Success)
        } else {
            Ok(AuthResult::Failure { pam_message: None })
        }
    }
}

fn authenticate_with_pam(
    service: &str,
    username: &str,
    password: SecretString,
) -> anyhow::Result<AuthResult> {
    let second_factor_seen = Rc::new(Cell::new(false));
    let last_message: Rc<Cell<Option<String>>> = Rc::new(Cell::new(None));
    let conversation = LockConversation::new(
        username,
        Some(password),
        second_factor_seen.clone(),
        last_message.clone(),
    );
    let mut context = Context::new(service, Some(username), conversation)?;
    match context.authenticate(Flag::DISALLOW_NULL_AUTHTOK) {
        Ok(()) => match context.acct_mgmt(Flag::NONE) {
            Ok(()) => Ok(AuthResult::Success),
            Err(error) => {
                let code = error.code();
                tracing::warn!(%error, ?code, "PAM account validation failed");
                Ok(account_failure_result(code, last_message.take()))
            }
        },
        Err(error) => {
            if second_factor_seen.get() {
                tracing::warn!(
                    %error,
                    "PAM stack requires a second factor; glimpse-lock does not support multi-prompt PAM stacks"
                );
                Ok(AuthResult::SecondFactorRequired)
            } else {
                let code = error.code();
                if let Some(unavailable) =
                    authenticate_unavailable_result(code, last_message.take())
                {
                    tracing::warn!(%error, ?code, "PAM account unavailable");
                    Ok(unavailable)
                } else {
                    tracing::warn!(%error, ?code, "PAM authentication failed");
                    Ok(AuthResult::Failure {
                        pam_message: last_message.take(),
                    })
                }
            }
        }
    }
}

fn account_failure_result(code: ErrorCode, pam_message: Option<String>) -> AuthResult {
    let reason = match code {
        ErrorCode::ACCT_EXPIRED => "Account expired",
        ErrorCode::NEW_AUTHTOK_REQD | ErrorCode::CRED_EXPIRED | ErrorCode::AUTHTOK_EXPIRED => {
            "Password change required to log in"
        }
        ErrorCode::PERM_DENIED => "Account access denied",
        ErrorCode::USER_UNKNOWN => "Account not found",
        _ => return AuthResult::Failure { pam_message },
    };
    AuthResult::AccountUnavailable {
        reason: reason.into(),
        pam_message,
    }
}

fn authenticate_unavailable_result(
    code: ErrorCode,
    pam_message: Option<String>,
) -> Option<AuthResult> {
    let reason = match code {
        ErrorCode::ACCT_EXPIRED => "Account expired",
        ErrorCode::USER_UNKNOWN => "Account not found",
        ErrorCode::MAXTRIES => "Too many attempts; try again later",
        ErrorCode::PERM_DENIED => "Account access denied",
        ErrorCode::AUTHINFO_UNAVAIL => "Authentication service unavailable",
        _ => return None,
    };
    Some(AuthResult::AccountUnavailable {
        reason: reason.into(),
        pam_message,
    })
}

struct LockConversation {
    username: String,
    // Wrapped in Option so prompt_echo_off can `take()` it on the first prompt:
    // the taken SecretString drops at the end of the callback and zeroes our
    // source buffer. The plaintext still lives momentarily inside the CString
    // we hand to pam_client2 (its Box<[u8]> is not zeroized on drop) and again
    // inside libpam's `strdup`'d response buffer — those two copies are outside
    // our control via the current pam_client2 API.
    password: Option<SecretString>,
    password_prompt_count: u32,
    second_factor_seen: Rc<Cell<bool>>,
    last_message: Rc<Cell<Option<String>>>,
}

impl LockConversation {
    fn new(
        username: &str,
        password: Option<SecretString>,
        second_factor_seen: Rc<Cell<bool>>,
        last_message: Rc<Cell<Option<String>>>,
    ) -> Self {
        Self {
            username: username.to_owned(),
            password,
            password_prompt_count: 0,
            second_factor_seen,
            last_message,
        }
    }

    fn capture(&self, msg: &CStr) {
        let text = msg.to_string_lossy().trim().to_owned();
        if !text.is_empty() {
            self.last_message.set(Some(text));
        }
    }
}

impl ConversationHandler for LockConversation {
    fn prompt_echo_on(&mut self, _prompt: &CStr) -> Result<CString, ErrorCode> {
        CString::new(self.username.clone()).map_err(|_| ErrorCode::CONV_ERR)
    }

    fn prompt_echo_off(&mut self, _prompt: &CStr) -> Result<CString, ErrorCode> {
        self.password_prompt_count += 1;
        if self.password_prompt_count == 1 {
            // take() leaves None; the SecretString drops at the end of this
            // scope, zeroing our copy of the password as soon as it's been
            // copied into the CString that pam_client2 forwards to libpam.
            let password = self.password.take().ok_or(ErrorCode::CONV_ERR)?;
            CString::new(password.as_str()).map_err(|_| ErrorCode::CONV_ERR)
        } else {
            self.second_factor_seen.set(true);
            Err(ErrorCode::CONV_ERR)
        }
    }

    fn text_info(&mut self, msg: &CStr) {
        tracing::debug!(message = %msg.to_string_lossy(), "PAM info");
        self.capture(msg);
    }

    fn error_msg(&mut self, msg: &CStr) {
        tracing::debug!(message = %msg.to_string_lossy(), "PAM error");
        self.capture(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthResult, Authenticator, PreviewAuthenticator, SecretString};

    #[test]
    fn preview_authenticator_accepts_valid_password_only() {
        let authenticator = PreviewAuthenticator::default();

        assert_eq!(
            authenticator
                .authenticate("unused", "preview", SecretString::new("valid"))
                .expect("preview auth should not fail"),
            AuthResult::Success
        );
        assert_eq!(
            authenticator
                .authenticate("unused", "preview", SecretString::new("invalid"))
                .expect("preview auth should not fail"),
            AuthResult::Failure { pam_message: None }
        );
    }
}
