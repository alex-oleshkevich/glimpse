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
