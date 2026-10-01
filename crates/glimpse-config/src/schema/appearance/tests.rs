use super::{Appearance, BlurSurface};

#[test]
fn blur_names_the_surfaces_it_applies_to() {
    let parsed: Appearance =
        toml::from_str("blur = [\"panel\", \"notification\"]\n").expect("the surfaces parse");

    assert_eq!(parsed.blur, [BlurSurface::Panel, BlurSurface::Notification]);
}

#[test]
fn blur_is_off_until_asked_for() {
    assert!(Appearance::default().blur.is_empty());
}

#[test]
fn animation_speed_is_normal_until_changed_and_zero_is_allowed() {
    assert_eq!(Appearance::default().animation_speed, 1.0);
    let off: Appearance = toml::from_str("animation-speed = 0\n").expect("zero parses");
    assert_eq!(off.animation_speed, 0.0);
    let fast: Appearance = toml::from_str("animation-speed = 1.5\n").expect("a fraction parses");
    assert_eq!(fast.animation_speed, 1.5);
}

#[test]
fn animation_speed_rejects_a_negative_a_crawl_an_absurd_and_a_non_number() {
    for text in [
        "animation-speed = -1\n",
        "animation-speed = 11\n",
        "animation-speed = nan\n",
        "animation-speed = 0.01\n",
    ] {
        assert!(toml::from_str::<Appearance>(text).is_err(), "{text} parsed");
    }
}

#[test]
fn blur_rejects_a_surface_it_does_not_know() {
    assert!(toml::from_str::<Appearance>("blur = [\"lock\"]\n").is_err());
}
