use super::*;

fn argv_of(program: &str) -> Vec<String> {
    vec![program.to_owned()]
}

#[test]
fn every_gesture_runs_its_own_program() {
    let config = CommandAppletConfig {
        on_click: argv_of("left"),
        on_middle_click: argv_of("middle"),
        on_right_click: argv_of("right"),
        on_scroll_up: argv_of("up"),
        on_scroll_down: argv_of("down"),
        on_scroll_left: argv_of("west"),
        on_scroll_right: argv_of("east"),
        ..Default::default()
    };
    let cases = [
        (Pointer::Press(Button::Left), "left"),
        (Pointer::Press(Button::Middle), "middle"),
        (Pointer::Press(Button::Right), "right"),
        (Pointer::Scroll(Direction::Up), "up"),
        (Pointer::Scroll(Direction::Down), "down"),
        (Pointer::Scroll(Direction::Left), "west"),
        (Pointer::Scroll(Direction::Right), "east"),
    ];
    for (pointer, program) in cases {
        assert_eq!(argv(&config, pointer), [program], "{pointer:?}");
    }
    assert!(argv(&config, Pointer::Press(Button::Other(8))).is_empty());
}

#[test]
fn an_unconfigured_gesture_runs_nothing() {
    let config = CommandAppletConfig::default();
    assert!(argv(&config, Pointer::Press(Button::Left)).is_empty());
    assert!(launch(argv(&config, Pointer::Press(Button::Left))).is_ok());
}

#[test]
fn an_icon_is_a_theme_name_unless_it_is_a_path() {
    assert!(icon("").is_none());
    assert!(icon("camera-photo-symbolic").is_some_and(|icon| icon.is::<gio::ThemedIcon>()));
    let present = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    assert!(icon(present).is_some_and(|icon| icon.is::<gio::FileIcon>()));
}

#[test]
fn a_missing_image_falls_back_to_the_default_icon() {
    let missing = icon("/nonexistent/glimpse/shot.png")
        .and_then(|icon| icon.downcast::<gio::ThemedIcon>().ok())
        .expect("a themed fallback");
    assert_eq!(
        missing.names().first().map(|name| name.as_str()),
        Some(FALLBACK_ICON)
    );
}

#[test]
fn a_repeated_failure_is_reported_once_per_quiet_period() {
    let now = Instant::now();
    let reported = ("pamixer".to_owned(), now);
    assert!(!quiet(None, "pamixer", now));
    assert!(quiet(
        Some(&reported),
        "pamixer",
        now + Duration::from_secs(1)
    ));
    assert!(!quiet(Some(&reported), "grim", now));
    assert!(!quiet(Some(&reported), "pamixer", now + REPORT_QUIET));
}

#[tokio::test]
async fn a_chip_with_neither_icon_nor_label_takes_no_room() {
    let services = crate::services::PanelServices::start_with_buses(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let mut command = Command::start(services.notifications());
    assert!(command.indicators().is_empty());

    command.label = Some("Shot".to_owned());
    let specs = command.indicators();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].label.as_deref(), Some("Shot"));
    assert!(specs[0].icon.is_none());

    services.shutdown().await;
}

#[test]
fn a_launch_that_cannot_start_is_an_error() {
    assert!(launch(&argv_of("glimpse-no-such-program")).is_err());
}
