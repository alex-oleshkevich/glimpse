use super::*;

fn output(connector: &str, built_in: bool) -> OutputInfo {
    OutputInfo {
        connector: connector.to_owned(),
        label: None,
        built_in,
        focused: false,
        make: None,
        model: None,
        serial: None,
        current_mode: None,
        logical: None,
        enabled: true,
    }
}

#[test]
fn the_widget_display_carries_whether_the_output_is_built_in() {
    assert!(to_widget_display(&output("eDP-1", true)).built_in);
    assert!(!to_widget_display(&output("DP-2", false)).built_in);
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
    display.outputs = vec![output("DP-1", false)];
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
