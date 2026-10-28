use super::*;

fn output(connector: &str) -> OutputInfo {
    OutputInfo {
        connector: connector.to_owned(),
        label: None,
        built_in: false,
        focused: false,
        make: None,
        model: None,
        current_mode: None,
        logical: None,
        enabled: true,
    }
}

#[tokio::test]
async fn brightness_without_a_current_source_uses_the_output_tooltip() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let mut display = Display::start(
        services.handles.brightness.clone(),
        services.handles.night_light.clone(),
        services.handles.compositor.clone(),
        services.handles.notifications.clone(),
    );
    display.outputs = vec![output("DP-1")];
    display.night_light_seen = true;
    assert_eq!(
        display.indicator().unwrap().tooltip,
        render::tooltip(&display.outputs)
    );
    assert!(display.indicator().unwrap().tooltip.is_some());

    display
        .state
        .sources
        .push(glimpse_services::BrightnessSource {
            id: "keyboard".to_owned(),
            kind: glimpse_services::BrightnessKind::Keyboard,
            connector: None,
            current: 1,
            max: 3,
            floor: 0,
        });
    display.night_light_seen = false;
    assert_eq!(
        display.indicator().unwrap().tooltip,
        render::tooltip(&display.outputs)
    );
    services.shutdown().await;
}
