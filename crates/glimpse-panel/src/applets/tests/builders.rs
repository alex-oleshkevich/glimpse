use super::*;

#[tokio::test]
async fn the_session_applet_needs_the_dialog_host() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let config: AppletConfig = AppletKind::Session {
        tooltip_format: None,
    }
    .into();
    let (dialog, _rx) = relm4::channel();

    assert!(
        build(&config, &services.handles, test_placement(), None,).is_none(),
        "a session chip cannot confirm without the host"
    );
    assert!(
        build(&config, &services.handles, test_placement(), Some(&dialog),).is_some(),
        "session is a configured panel chip"
    );

    services.shutdown().await;
}
