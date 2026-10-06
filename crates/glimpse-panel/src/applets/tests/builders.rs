use super::*;

#[tokio::test]
async fn the_printing_applet_now_produces_a_builder() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let config: AppletConfig = AppletKind::Printing(<_>::default()).into();

    let built = build(
        &config,
        &services.compositor,
        &services.keyboard,
        &services.calendar,
        &services.mpris,
        &services.heartbeat,
        &services.tray,
        &services.bluetooth,
        &services.network,
        &services.audio,
        &services.brightness,
        &services.night_light(),
        &services.notifications(),
        &services.weather(),
        &services.idle(),
        &services.color_picker,
        &services.session_actions,
        &services.battery,
        &services.clipboard,
        &services.places,
        &services.printing,
        &services.removable,
        &services.kdeconnect,
        &services.privacy,
        &services.system_monitor,
        &services.ruler,
        &services.exec(),
        test_placement(),
        None,
    );
    assert!(built.is_some(), "printing now has an implementation");

    services.shutdown().await;
}

#[tokio::test]
async fn the_privacy_applet_now_produces_a_builder() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let config: AppletConfig = AppletKind::Privacy(<_>::default()).into();

    let built = build(
        &config,
        &services.compositor,
        &services.keyboard,
        &services.calendar,
        &services.mpris,
        &services.heartbeat,
        &services.tray,
        &services.bluetooth,
        &services.network,
        &services.audio,
        &services.brightness,
        &services.night_light(),
        &services.notifications(),
        &services.weather(),
        &services.idle(),
        &services.color_picker,
        &services.session_actions,
        &services.battery,
        &services.clipboard,
        &services.places,
        &services.printing,
        &services.removable,
        &services.kdeconnect,
        &services.privacy,
        &services.system_monitor,
        &services.ruler,
        &services.exec(),
        test_placement(),
        None,
    );
    assert!(built.is_some(), "privacy now has an implementation");

    services.shutdown().await;
}

#[tokio::test]
async fn the_clipboard_applet_now_produces_a_builder() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let config: AppletConfig = AppletKind::Clipboard(<_>::default()).into();

    let built = build(
        &config,
        &services.compositor,
        &services.keyboard,
        &services.calendar,
        &services.mpris,
        &services.heartbeat,
        &services.tray,
        &services.bluetooth,
        &services.network,
        &services.audio,
        &services.brightness,
        &services.night_light(),
        &services.notifications(),
        &services.weather(),
        &services.idle(),
        &services.color_picker,
        &services.session_actions,
        &services.battery,
        &services.clipboard,
        &services.places,
        &services.printing,
        &services.removable,
        &services.kdeconnect,
        &services.privacy,
        &services.system_monitor,
        &services.ruler,
        &services.exec(),
        test_placement(),
        None,
    );
    assert!(built.is_some(), "clipboard now has an implementation");

    services.shutdown().await;
}

#[tokio::test]
async fn the_places_applet_is_built_and_renders_a_chip() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let config = configured("places", &BTreeMap::new(), &Regional::default())
        .expect("`places` is a known applet");

    let built = build(
        &config,
        &services.compositor,
        &services.keyboard,
        &services.calendar,
        &services.mpris,
        &services.heartbeat,
        &services.tray,
        &services.bluetooth,
        &services.network,
        &services.audio,
        &services.brightness,
        &services.night_light(),
        &services.notifications(),
        &services.weather(),
        &services.idle(),
        &services.color_picker,
        &services.session_actions,
        &services.battery,
        &services.clipboard,
        &services.places,
        &services.printing,
        &services.removable,
        &services.kdeconnect,
        &services.privacy,
        &services.system_monitor,
        &services.ruler,
        &services.exec(),
        test_placement(),
        None,
    );
    assert!(built.is_some(), "places has an implementation");

    services.shutdown().await;
}

#[tokio::test]
async fn the_removable_applet_is_built_from_its_own_name() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let config = configured("removable", &BTreeMap::new(), &Regional::default())
        .expect("`removable` is a known applet");

    let built = build(
        &config,
        &services.compositor,
        &services.keyboard,
        &services.calendar,
        &services.mpris,
        &services.heartbeat,
        &services.tray,
        &services.bluetooth,
        &services.network,
        &services.audio,
        &services.brightness,
        &services.night_light(),
        &services.notifications(),
        &services.weather(),
        &services.idle(),
        &services.color_picker,
        &services.session_actions,
        &services.battery,
        &services.clipboard,
        &services.places,
        &services.printing,
        &services.removable,
        &services.kdeconnect,
        &services.privacy,
        &services.system_monitor,
        &services.ruler,
        &services.exec(),
        test_placement(),
        None,
    );
    assert!(built.is_some(), "removable has an implementation");

    services.shutdown().await;
}

#[tokio::test]
async fn the_system_monitor_applet_produces_a_builder() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let config = configured("system-monitor", &BTreeMap::new(), &Regional::default())
        .expect("`system-monitor` is a known applet");

    let built = build(
        &config,
        &services.compositor,
        &services.keyboard,
        &services.calendar,
        &services.mpris,
        &services.heartbeat,
        &services.tray,
        &services.bluetooth,
        &services.network,
        &services.audio,
        &services.brightness,
        &services.night_light(),
        &services.notifications(),
        &services.weather(),
        &services.idle(),
        &services.color_picker,
        &services.session_actions,
        &services.battery,
        &services.clipboard,
        &services.places,
        &services.printing,
        &services.removable,
        &services.kdeconnect,
        &services.privacy,
        &services.system_monitor,
        &services.ruler,
        &services.exec(),
        test_placement(),
        None,
    );
    assert!(built.is_some(), "system-monitor has an implementation");

    services.shutdown().await;
}

#[tokio::test]
async fn the_audio_applet_now_produces_a_builder() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let config: AppletConfig = AppletKind::Audio {}.into();

    let built = build(
        &config,
        &services.compositor,
        &services.keyboard,
        &services.calendar,
        &services.mpris,
        &services.heartbeat,
        &services.tray,
        &services.bluetooth,
        &services.network,
        &services.audio,
        &services.brightness,
        &services.night_light(),
        &services.notifications(),
        &services.weather(),
        &services.idle(),
        &services.color_picker,
        &services.session_actions,
        &services.battery,
        &services.clipboard,
        &services.places,
        &services.printing,
        &services.removable,
        &services.kdeconnect,
        &services.privacy,
        &services.system_monitor,
        &services.ruler,
        &services.exec(),
        test_placement(),
        None,
    );
    assert!(built.is_some(), "audio now has an implementation");

    services.shutdown().await;
}

#[tokio::test]
async fn the_session_applet_needs_the_dialog_host() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let config: AppletConfig = AppletKind::Session {}.into();
    let (dialog, _rx) = relm4::channel();

    assert!(
        build(
            &config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            &services.exec(),
            test_placement(),
            None,
        )
        .is_none(),
        "a session chip cannot confirm without the host"
    );
    assert!(
        build(
            &config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            &services.exec(),
            test_placement(),
            Some(&dialog),
        )
        .is_some(),
        "session is a configured panel chip"
    );

    services.shutdown().await;
}

#[tokio::test]
async fn the_display_and_idle_applets_produce_a_builder() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let display_config: AppletConfig = AppletKind::Display(<_>::default()).into();
    let idle_config: AppletConfig = AppletKind::Idle {}.into();
    let power_config: AppletConfig = AppletKind::Power(<_>::default()).into();
    let command_config: AppletConfig = AppletKind::Command(<_>::default()).into();
    let ruler_config: AppletConfig = AppletKind::Ruler {}.into();

    for config in [
        &display_config,
        &idle_config,
        &power_config,
        &command_config,
        &ruler_config,
    ] {
        let built = build(
            config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            &services.exec(),
            test_placement(),
            None,
        );
        assert!(
            built.is_some(),
            "each configured panel chip must build from one config"
        );
    }

    services.shutdown().await;
}
