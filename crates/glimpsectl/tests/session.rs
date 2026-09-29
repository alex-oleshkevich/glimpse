use glimpsectl::commands::session::should_declare;

#[test]
fn only_a_plain_niri_session_is_declared() {
    assert!(!should_declare(None));
    assert!(!should_declare(Some("glimpse:niri")));
    assert!(!should_declare(Some("Hyprland")));
    assert!(!should_declare(Some("niri:x")));
    assert!(!should_declare(Some("")));
    assert!(should_declare(Some("niri")));
}
