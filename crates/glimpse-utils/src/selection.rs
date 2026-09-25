/// The mime an application sets beside its content to say the clipboard holds a secret. Matched
/// case-insensitively because it is a convention rather than a registered type, and the
/// applications that honour it do not agree on the spelling.
const SENSITIVE_HINT: &str = "x-kde-passwordManagerHint";

/// Whether an offer's mime list asks not to be remembered. This is the whole of the
/// password-manager rule and the one place it is decided; a backend classifies the offer with it
/// before reading any content, so a secret is never held even briefly.
pub fn is_sensitive(mimes: &[String]) -> bool {
    mimes
        .iter()
        .any(|mime| mime.trim().eq_ignore_ascii_case(SENSITIVE_HINT))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mimes(list: &[&str]) -> Vec<String> {
        list.iter().map(|mime| (*mime).to_owned()).collect()
    }

    /// The spelling is a convention, not a registered type, and the applications that set it do
    /// not agree on its casing.
    #[test]
    fn the_password_hint_is_recognised_in_any_casing() {
        for spelling in [
            "x-kde-passwordManagerHint",
            "x-kde-passwordmanagerhint",
            "X-KDE-PASSWORDMANAGERHINT",
            "  x-kde-PasswordManagerHint  ",
        ] {
            assert!(
                is_sensitive(&mimes(&["text/plain", spelling])),
                "{spelling} must be recognised"
            );
        }
    }

    #[test]
    fn an_ordinary_offer_is_not_sensitive() {
        assert!(!is_sensitive(&mimes(&["text/plain", "UTF8_STRING"])));
        assert!(!is_sensitive(&[]));
        assert!(
            !is_sensitive(&mimes(&["x-kde-passwordManagerHintish"])),
            "a longer mime that merely starts the same is a different type"
        );
    }
}
