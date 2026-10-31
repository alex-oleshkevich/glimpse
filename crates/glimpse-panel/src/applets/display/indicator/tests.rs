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

fn started() -> (crate::services::PanelServices, Display) {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let display = Display::start(
        services.handles.brightness.clone(),
        services.handles.night_light.clone(),
        services.handles.compositor.clone(),
        services.handles.notifications.clone(),
    );
    (services, display)
}

fn panel(current: u32) -> glimpse_services::BrightnessSource {
    glimpse_services::BrightnessSource {
        id: "panel".to_owned(),
        kind: glimpse_services::BrightnessKind::Display,
        connector: None,
        current,
        max: 100,
        floor: 0,
    }
}

#[tokio::test]
async fn brightness_without_a_current_source_uses_the_output_tooltip() {
    let (services, mut display) = started();
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

#[tokio::test]
async fn the_slider_shows_the_newest_level_while_a_write_is_busy() {
    let (services, display) = started();
    let coalescer = Rc::new(RefCell::new(Coalescer::new()));
    display
        .level_coalescers
        .borrow_mut()
        .insert("panel".to_owned(), Rc::clone(&coalescer));
    assert_eq!(coalescer.borrow_mut().request(10), Some(10));
    assert_eq!(coalescer.borrow_mut().request(30), None);

    assert_eq!(
        display.level(&panel(10)),
        30,
        "the published 10 lags the request still waiting behind the busy write"
    );

    assert_eq!(coalescer.borrow_mut().completed(), Some(30));
    assert_eq!(coalescer.borrow_mut().completed(), None);
    assert_eq!(
        display.level(&panel(42)),
        42,
        "once idle the published level is followed again"
    );
    services.shutdown().await;
}

#[tokio::test]
async fn a_stale_night_light_publish_during_the_glide_keeps_the_requested_temperature() {
    let (services, display) = started();
    display
        .pending_temperature
        .set(Some((4000, Instant::now())));

    assert_eq!(display.temperature(5200), 4000);
    assert_eq!(display.temperature(4600), 4000);
    assert_eq!(display.temperature(4000), 4000);
    assert_eq!(display.pending_temperature.get(), None);
    assert_eq!(
        display.temperature(5200),
        5200,
        "once reached, publishes are followed again"
    );
    services.shutdown().await;
}

#[tokio::test]
async fn a_requested_temperature_gives_way_once_the_glide_window_has_passed() {
    let (services, display) = started();
    let long_ago = Instant::now()
        .checked_sub(TEMPERATURE_SETTLE + Duration::from_millis(1))
        .expect("the clock is past the glide window");
    display.pending_temperature.set(Some((4000, long_ago)));

    assert_eq!(display.temperature(5200), 5200);
    assert_eq!(display.pending_temperature.get(), None);
    services.shutdown().await;
}
