use super::*;

#[test]
fn backdrop_is_created_only_on_niri() {
    assert!(wants_role(Role::Wallpaper, false));
    assert!(wants_role(Role::Wallpaper, true));
    assert!(!wants_role(Role::Backdrop, false));
    assert!(wants_role(Role::Backdrop, true));
}
