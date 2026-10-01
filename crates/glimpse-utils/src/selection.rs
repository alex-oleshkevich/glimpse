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
mod tests;
