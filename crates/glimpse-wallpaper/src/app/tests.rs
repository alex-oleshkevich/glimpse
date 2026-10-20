use super::*;

#[test]
fn backdrop_is_created_only_where_the_compositor_has_an_overview_backdrop() {
    assert!(wants_role(Role::Wallpaper, false));
    assert!(wants_role(Role::Wallpaper, true));
    assert!(!wants_role(Role::Backdrop, false));
    assert!(wants_role(Role::Backdrop, true));
}
