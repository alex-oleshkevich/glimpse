use super::*;

#[test]
fn is_stale_session_matches_the_four_stale_names() {
    assert!(is_stale_session(&method_error(
        "org.freedesktop.DBus.Error.UnknownObject"
    )));
    assert!(is_stale_session(&method_error(
        "org.freedesktop.login1.NoSuchSession"
    )));
    assert!(is_stale_session(&method_error(
        "org.freedesktop.DBus.Error.ServiceUnknown"
    )));
    assert!(is_stale_session(&method_error(
        "org.freedesktop.DBus.Error.NameHasNoOwner"
    )));
}

#[test]
fn is_stale_session_ignores_an_unrelated_method_error() {
    assert!(!is_stale_session(&method_error(
        "org.freedesktop.DBus.Error.AccessDenied"
    )));
}

#[test]
fn is_stale_session_ignores_a_non_method_error_variant() {
    assert!(!is_stale_session(&zbus::Error::Unsupported));
}
