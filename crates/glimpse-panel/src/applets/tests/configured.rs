use super::*;

#[test]
fn every_name_in_the_default_zones_resolves_to_a_kind() {
    let panel = glimpse_config::Panel::default();
    for name in panel.left.iter().chain(&panel.center).chain(&panel.right) {
        assert!(
            configured(name, &BTreeMap::new(), &Regional::default()).is_some(),
            "`{name}` ships in a default zone, so an untouched installation must resolve it"
        );
    }
}

#[test]
fn a_known_name_resolves_to_configuration() {
    assert!(configured("heartbeat", &BTreeMap::new(), &Regional::default()).is_some());
}

#[test]
fn extends_names_the_kind_so_one_kind_can_have_several_instances() {
    let applets = custom("pulse", AppletKind::Heartbeat {}.into());
    assert!(
        configured("pulse", &applets, &Regional::default()).is_some(),
        "`pulse` is not a kind; `extends` is what says which one it is"
    );
    assert!(
        configured("pulse", &BTreeMap::new(), &Regional::default()).is_none(),
        "without the entry the same name is just unknown"
    );
}

#[test]
fn the_next_event_applet_is_configured_rather_than_skipped() {
    assert!(
        configured("next-event", &BTreeMap::new(), &Regional::default()).is_some(),
        "the kind is known, so it must not be treated as a typo"
    );
}

#[test]
fn the_tray_applet_is_configured_and_no_longer_falls_through_to_nothing() {
    let applets = configured("tray", &BTreeMap::new(), &Regional::default())
        .expect("`tray` ships in the default right zone, so it must resolve");
    assert!(
        matches!(applets.kind, AppletKind::Tray(_)),
        "a unit variant would let deny_unknown_fields swallow every key under it"
    );
}

#[test]
fn the_weather_applet_is_configured_rather_than_skipped() {
    assert!(
        configured("weather", &BTreeMap::new(), &Regional::default()).is_some(),
        "the kind is known, so it must not be treated as a typo"
    );
}

#[test]
fn the_keyboard_applet_is_configured_rather_than_skipped() {
    assert!(
        configured("keyboard", &BTreeMap::new(), &Regional::default()).is_some(),
        "the kind is known, so it must not be treated as a typo"
    );
}

#[test]
fn the_clipboard_applet_is_configured_and_no_longer_falls_through_to_nothing() {
    let applets = configured("clipboard", &BTreeMap::new(), &Regional::default())
        .expect("`clipboard` ships in the default right zone, so it must resolve");
    assert!(
        matches!(applets.kind, AppletKind::Clipboard(_)),
        "a unit variant would let deny_unknown_fields swallow every key under it"
    );
}

#[test]
fn the_notifications_applet_is_configured_rather_than_skipped() {
    assert!(
        configured("notifications", &BTreeMap::new(), &Regional::default()).is_some(),
        "the kind is known, so it must not be treated as a typo"
    );
}

#[test]
fn a_kind_without_an_implementation_is_not_the_same_as_a_typo() {
    assert!(
        AppletConfig::from_name("removable").is_some(),
        "`removable` is a real applet, so skipping it is expected rather than a bad document"
    );
    assert!(AppletConfig::from_name("nonesuch").is_none());
    assert!(configured("nonesuch", &BTreeMap::new(), &Regional::default()).is_none());
}

/// The shipped defaults name applets in a panel zone and carry no `[applets.<name>]` table for
/// them, so every applet on a default bar is built by `from_name` rather than found in the
/// map. That path invents a config, and an invented config that kept `Regional::default()`
/// would silently ignore an explicit `hour-format`.
#[test]
fn an_applet_with_no_table_of_its_own_still_carries_the_document_s_regional() {
    let twelve = Regional {
        hour_format: glimpse_config::HourFormat::Twelve,
        ..Regional::default()
    };

    let built = configured("clock", &BTreeMap::new(), &twelve).expect("the clock configures");
    assert!(
        built.regional.twelve_hour(),
        "an applet with no table of its own must still read the document's `[regional]`"
    );

    let found = configured(
        "clock",
        &custom("clock", AppletKind::Clock(<_>::default()).into()),
        &twelve,
    )
    .expect("the clock configures");
    assert!(
        found.regional.twelve_hour(),
        "and so must one that has a table"
    );
}
