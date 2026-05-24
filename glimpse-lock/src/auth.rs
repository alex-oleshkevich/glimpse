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

// Clone is required because LockWindowInput::Submit(SecretString) derives
// Clone for relm4's message broadcasting machinery. The Submit variant is
// emitted from LockWindow -> LockApp (single sender, single receiver), so
// in practice clones never happen on the password path. If LockWindowInput
// is ever split into separate inbound/outbound enums, drop this impl.
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
    let last_message: Rc<Cell<Option<String>>> = Rc::new(Cell::new(None));
    let conversation = LockConversation::new(username, Some(password), last_message.clone());
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

/// Maps an `ErrorCode` returned by `pam_acct_mgmt()` into the user-visible
/// `AuthResult`. This path is reached when `pam_authenticate` succeeded
/// (the password was correct) but the account-management chain rejected
/// the session.
///
/// The code set here is intentionally a subset of
/// [`authenticate_unavailable_result`]: codes like `MAXTRIES` and
/// `AUTHINFO_UNAVAIL` only flow out of `pam_authenticate`, never out of
/// `pam_acct_mgmt`, so they have no entry here. Conversely
/// `NEW_AUTHTOK_REQD` / `CRED_EXPIRED` / `AUTHTOK_EXPIRED` only surface
/// at the account-management phase and so are unique to this side.
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

/// Maps an `ErrorCode` returned by `pam_authenticate()` into an optional
/// "account is unavailable" `AuthResult`. Returns `None` for codes that
/// should fall through to a plain `Failure { pam_message }` (most notably
/// `AUTH_ERR`, which is just "wrong password").
///
/// The code set here is intentionally a subset of
/// [`account_failure_result`]: see that function's docs for the rationale.
/// Notable codes unique to this side:
///   * `MAXTRIES`           — pam_faillock signalled too many recent failures
///   * `AUTHINFO_UNAVAIL`   — backend (e.g. unix_chkpwd, SSSD) unreachable
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
    last_message: Rc<Cell<Option<String>>>,
}

impl LockConversation {
    fn new(
        username: &str,
        password: Option<SecretString>,
        last_message: Rc<Cell<Option<String>>>,
    ) -> Self {
        Self {
            username: username.to_owned(),
            password,
            password_prompt_count: 0,
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
            // Any second echo-off prompt is reported as a conversation error.
            // We deliberately do NOT try to detect "second factor required"
            // here: a PAM module retrying a single prompt (transient EINTR,
            // SSSD reconnects, etc.) looks identical to a real MFA prompt.
            // Surfacing it as plain failure keeps the message honest; any text
            // PAM printed via text_info/error_msg before this point will be
            // captured in last_message and shown to the user.
            tracing::debug!(
                count = self.password_prompt_count,
                "PAM requested an additional echo-off prompt; treating as failure"
            );
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
    use super::{
        AuthResult, Authenticator, ErrorCode, PreviewAuthenticator, SecretString,
        account_failure_result, authenticate_unavailable_result,
    };

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

    fn unavailable(reason: &str, pam_message: Option<&str>) -> AuthResult {
        AuthResult::AccountUnavailable {
            reason: reason.into(),
            pam_message: pam_message.map(str::to_owned),
        }
    }

    fn failure(pam_message: Option<&str>) -> AuthResult {
        AuthResult::Failure {
            pam_message: pam_message.map(str::to_owned),
        }
    }

    /// Table-driven test pinning the post-acct_mgmt mapping. These mappings
    /// are user-visible (they drive the status message) so changes must be
    /// deliberate.
    #[test]
    fn account_failure_result_maps_known_codes() {
        let cases: &[(ErrorCode, AuthResult)] = &[
            (
                ErrorCode::ACCT_EXPIRED,
                unavailable("Account expired", None),
            ),
            (
                ErrorCode::NEW_AUTHTOK_REQD,
                unavailable("Password change required to log in", None),
            ),
            (
                ErrorCode::CRED_EXPIRED,
                unavailable("Password change required to log in", None),
            ),
            (
                ErrorCode::AUTHTOK_EXPIRED,
                unavailable("Password change required to log in", None),
            ),
            (
                ErrorCode::PERM_DENIED,
                unavailable("Account access denied", None),
            ),
            (
                ErrorCode::USER_UNKNOWN,
                unavailable("Account not found", None),
            ),
            // Any other code falls through to Failure (e.g. pam_faillock
            // returning AUTH_ERR from its account phase when the account is
            // currently locked).
            (ErrorCode::AUTH_ERR, failure(None)),
            (ErrorCode::ABORT, failure(None)),
        ];
        for (code, expected) in cases {
            assert_eq!(
                account_failure_result(*code, None),
                *expected,
                "account_failure_result mapping mismatch for {code:?}"
            );
        }
    }

    /// Same table-driven approach for the authenticate() failure path. Note
    /// the slightly different code set (e.g. MAXTRIES and AUTHINFO_UNAVAIL
    /// are recognised here but not by account_failure_result).
    #[test]
    fn authenticate_unavailable_result_maps_known_codes() {
        let cases: &[(ErrorCode, Option<AuthResult>)] = &[
            (
                ErrorCode::ACCT_EXPIRED,
                Some(unavailable("Account expired", None)),
            ),
            (
                ErrorCode::USER_UNKNOWN,
                Some(unavailable("Account not found", None)),
            ),
            (
                ErrorCode::MAXTRIES,
                Some(unavailable("Too many attempts; try again later", None)),
            ),
            (
                ErrorCode::PERM_DENIED,
                Some(unavailable("Account access denied", None)),
            ),
            (
                ErrorCode::AUTHINFO_UNAVAIL,
                Some(unavailable("Authentication service unavailable", None)),
            ),
            // Unmapped codes return None so the caller can render a generic
            // "Authentication failed" / cooldown message.
            (ErrorCode::AUTH_ERR, None),
            (ErrorCode::ABORT, None),
        ];
        for (code, expected) in cases {
            assert_eq!(
                authenticate_unavailable_result(*code, None),
                *expected,
                "authenticate_unavailable_result mapping mismatch for {code:?}"
            );
        }
    }

    #[test]
    fn account_failure_result_threads_pam_message_through() {
        let result = account_failure_result(ErrorCode::ACCT_EXPIRED, Some("Custom reason".into()));
        assert_eq!(
            result,
            unavailable("Account expired", Some("Custom reason"))
        );
    }

    #[test]
    fn authenticate_unavailable_result_threads_pam_message_through() {
        let result =
            authenticate_unavailable_result(ErrorCode::MAXTRIES, Some("3 attempts remaining".into()));
        assert_eq!(
            result,
            Some(unavailable(
                "Too many attempts; try again later",
                Some("3 attempts remaining")
            ))
        );
    }
}
