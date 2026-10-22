use super::{Appearance, ColorScheme, DesktopColorScheme};

#[test]
fn desktop_color_scheme_defaults_to_sun_and_accepts_only_its_policies() {
    assert_eq!(
        toml::from_str::<Appearance>("")
            .unwrap()
            .desktop_color_scheme,
        DesktopColorScheme::Sun
    );
    for (value, expected) in [
        ("sun", DesktopColorScheme::Sun),
        ("light", DesktopColorScheme::Light),
        ("dark", DesktopColorScheme::Dark),
        ("external", DesktopColorScheme::External),
    ] {
        let parsed: Appearance =
            toml::from_str(&format!("desktop-color-scheme = {value:?}")).unwrap();
        assert_eq!(parsed.desktop_color_scheme, expected);
        assert_eq!(parsed.color_scheme, ColorScheme::Dark);
    }
    for value in ["auto", "system", "solar", "off", "schedule"] {
        assert!(
            toml::from_str::<Appearance>(&format!("desktop-color-scheme = {value:?}")).is_err()
        );
    }
}

#[test]
fn the_color_scheme_schema_lists_light_dark_and_desktop() {
    let schema: serde_json::Value =
        serde_json::from_str(&crate::json_schema_document(crate::Documents::Config)).unwrap();
    assert_eq!(
        schema["$defs"]["ColorScheme"]["enum"],
        serde_json::json!(["light", "dark", "desktop"])
    );
}

#[test]
fn glimpse_color_scheme_accepts_desktop_light_and_dark() {
    for (value, expected) in [
        ("desktop", ColorScheme::Desktop),
        ("light", ColorScheme::Light),
        ("dark", ColorScheme::Dark),
    ] {
        let parsed: Appearance = toml::from_str(&format!("color-scheme = {value:?}")).unwrap();
        assert_eq!(parsed.color_scheme, expected);
    }
}

#[test]
fn blur_is_a_boolean_that_defaults_to_off() {
    assert!(!Appearance::default().blur);
    let on: Appearance = toml::from_str("blur = true\n").expect("true parses");
    assert!(on.blur);
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
fn blur_rejects_the_removed_list_form() {
    assert!(toml::from_str::<Appearance>("blur = [\"all\"]\n").is_err());
    assert!(toml::from_str::<Appearance>("blur = [\"panel\"]\n").is_err());
}
