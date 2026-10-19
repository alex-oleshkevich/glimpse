use super::*;

#[test]
#[ignore = "needs a display"]
fn shot_icons() {
    shot_display();
    let display = gdk::Display::default().expect("a display");
    let theme = gtk4::IconTheme::for_display(&display);
    for name in [
        "shot-area-symbolic",
        "shot-crop-symbolic",
        "shot-arrow-symbolic",
        "shot-rect-symbolic",
        "shot-rect-filled-symbolic",
        "shot-oval-symbolic",
        "shot-number-symbolic",
        "shot-text-symbolic",
        "shot-blur-symbolic",
        "shot-window-symbolic",
    ] {
        assert!(theme.has_icon(name), "{name} is not in the icon theme");
    }
}
