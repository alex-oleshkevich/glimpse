use super::*;

fn usage(kind: PrivacyResource) -> ServiceUsage {
    ServiceUsage {
        kind,
        app: None,
        icon: None,
        detail: None,
        since: std::time::SystemTime::now(),
        stream_id: None,
        session: None,
    }
}

fn state(usages: Vec<ServiceUsage>) -> PrivacyState {
    PrivacyState { usages }
}

#[test]
fn an_elapsed_cast_reads_mm_ss_until_it_passes_an_hour() {
    let start = SystemTime::UNIX_EPOCH;
    let at = |secs| elapsed(start, start + std::time::Duration::from_secs(secs));
    assert_eq!(at(0), "00:00");
    assert_eq!(at(12), "00:12");
    assert_eq!(at(59), "00:59");
    assert_eq!(at(60), "01:00");
    assert_eq!(at(3599), "59:59");
    assert_eq!(at(3600), "1:00:00");
    assert_eq!(at(3672), "1:01:12");
}

#[test]
fn a_clock_that_jumped_backwards_reads_zero_rather_than_panicking() {
    let now = SystemTime::UNIX_EPOCH;
    let future = now + std::time::Duration::from_secs(90);
    assert_eq!(elapsed(future, now), "00:00");
}

#[test]
fn several_casts_count_from_the_oldest_because_they_collapse_onto_one_chip() {
    let old = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
    let new = old + std::time::Duration::from_secs(50);
    let casts = state(vec![
        ServiceUsage {
            since: new,
            stream_id: Some(2),
            ..usage(PrivacyResource::Screen)
        },
        ServiceUsage {
            since: old,
            stream_id: Some(1),
            ..usage(PrivacyResource::Screen)
        },
    ]);

    assert_eq!(
        screencast_since(&casts, Filters::default()),
        Some(old),
        "the screen has been shared continuously since the first cast began"
    );
}

#[test]
fn a_hidden_screencast_starts_no_clock() {
    let casts = state(vec![usage(PrivacyResource::Screen)]);
    let filters = Filters {
        screencast: false,
        ..Filters::default()
    };

    assert_eq!(screencast_since(&casts, filters), None);
}

#[test]
fn detail_joins_app_and_detail_when_both_are_known() {
    let usage = ServiceUsage {
        app: Some("chrome".to_owned()),
        detail: Some("DP-2".to_owned()),
        ..usage(PrivacyResource::Screen)
    };
    assert_eq!(detail(&usage).as_deref(), Some("chrome · DP-2"));
}

#[test]
fn detail_falls_back_to_whichever_half_is_known() {
    let app_only = ServiceUsage {
        app: Some("chrome".to_owned()),
        ..usage(PrivacyResource::Microphone)
    };
    assert_eq!(detail(&app_only).as_deref(), Some("chrome"));

    let detail_only = ServiceUsage {
        detail: Some("DP-2".to_owned()),
        ..usage(PrivacyResource::Screen)
    };
    assert_eq!(detail(&detail_only).as_deref(), Some("DP-2"));
}

#[test]
fn a_location_usage_carries_neither_half_and_has_no_detail() {
    assert_eq!(detail(&usage(PrivacyResource::Location)), None);
}

#[test]
fn the_id_keys_a_screen_row_by_stream_id_not_by_session() {
    let a = ServiceUsage {
        stream_id: Some(3),
        session: Some(99),
        ..usage(PrivacyResource::Screen)
    };
    let b = ServiceUsage {
        stream_id: Some(3),
        session: None,
        ..usage(PrivacyResource::Screen)
    };
    assert_eq!(usage_id(&a), usage_id(&b));
}

#[test]
fn the_id_keys_camera_and_microphone_by_app() {
    let chrome = ServiceUsage {
        app: Some("chrome".to_owned()),
        ..usage(PrivacyResource::Camera)
    };
    let ffmpeg = ServiceUsage {
        app: Some("ffmpeg".to_owned()),
        ..usage(PrivacyResource::Camera)
    };
    assert_ne!(usage_id(&chrome), usage_id(&ffmpeg));
}

#[test]
fn location_always_has_the_same_id() {
    assert_eq!(
        usage_id(&usage(PrivacyResource::Location)),
        usage_id(&usage(PrivacyResource::Location))
    );
}

#[test]
fn a_filtered_out_resource_never_reaches_the_popover() {
    let filters = Filters {
        camera: false,
        ..Filters::default()
    };
    let usages = usages(
        &state(vec![usage(PrivacyResource::Camera)]),
        filters,
        &[],
        false,
    );
    assert!(usages.is_empty());
}

#[test]
fn a_filtered_out_resource_never_lights_a_chip() {
    let filters = Filters {
        location: false,
        ..Filters::default()
    };
    let kinds = kinds_in_use(&state(vec![usage(PrivacyResource::Location)]), filters);
    assert!(kinds.is_empty());
}

#[test]
fn no_usages_lights_no_chips() {
    assert!(kinds_in_use(&state(Vec::new()), Filters::default()).is_empty());
}

#[test]
fn every_kind_in_use_lights_its_own_chip() {
    let kinds = kinds_in_use(
        &state(vec![
            usage(PrivacyResource::Camera),
            usage(PrivacyResource::Screen),
        ]),
        Filters::default(),
    );
    assert_eq!(
        kinds,
        vec![PrivacyResource::Camera, PrivacyResource::Screen]
    );
}

fn output(connector: &str, label: Option<&str>, built_in: bool) -> OutputInfo {
    OutputInfo {
        connector: connector.to_owned(),
        label: label.map(str::to_owned),
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

fn chrome(kind: PrivacyResource) -> ServiceUsage {
    ServiceUsage {
        app: Some("Google Chrome".to_owned()),
        icon: Some("google-chrome".to_owned()),
        ..usage(kind)
    }
}

#[test]
fn one_app_using_several_things_is_one_row_that_names_them() {
    let outputs = [
        output("DP-2", Some("Dell U2723QE"), false),
        output("eDP-1", Some("Samsung"), true),
    ];
    let screen = ServiceUsage {
        detail: Some("DP-2".to_owned()),
        session: Some(7),
        stream_id: Some(1),
        ..chrome(PrivacyResource::Screen)
    };
    let rows = usages(
        &state(vec![
            chrome(PrivacyResource::Microphone),
            screen,
            chrome(PrivacyResource::Camera),
            usage(PrivacyResource::Location),
        ]),
        Filters::default(),
        &outputs,
        false,
    );
    assert_eq!(rows.len(), 2, "Chrome once, location once");
    assert_eq!(rows[0].title, "Google Chrome");
    assert_eq!(rows[0].icon, "google-chrome", "the app's own icon");
    assert_eq!(
        rows[0].detail.as_deref(),
        Some("Camera · Microphone · Sharing Dell U2723QE"),
        "resources read in a fixed order whatever order they arrived in"
    );
    assert!(rows[0].stoppable);
    assert_eq!(rows[1].title, "Location");
    assert_eq!(rows[1].detail, None);
    assert!(!rows[1].stoppable);
}

#[test]
fn an_unnamed_cast_is_titled_by_what_it_shares_and_says_where() {
    let outputs = [output("eDP-1", Some("Samsung"), true)];
    let cast = ServiceUsage {
        detail: Some("eDP-1".to_owned()),
        ..usage(PrivacyResource::Screen)
    };
    let window = ServiceUsage {
        stream_id: Some(2),
        ..usage(PrivacyResource::Screen)
    };
    let rows = usages(
        &state(vec![cast, window]),
        Filters::default(),
        &outputs,
        false,
    );
    assert_eq!(rows[0].title, "Screen");
    assert_eq!(rows[0].detail.as_deref(), Some("Built-in display"));
    assert_eq!(rows[1].detail.as_deref(), Some("a window"));
}

#[test]
fn a_muted_microphone_says_so_on_every_row_using_it() {
    let rows = usages(
        &state(vec![chrome(PrivacyResource::Microphone)]),
        Filters::default(),
        &[],
        true,
    );
    assert_eq!(rows[0].detail.as_deref(), Some("Microphone · muted"));
}

#[test]
fn stopping_a_row_stops_every_session_it_holds() {
    let one = ServiceUsage {
        session: Some(1),
        stream_id: Some(1),
        ..chrome(PrivacyResource::Screen)
    };
    let two = ServiceUsage {
        session: Some(2),
        stream_id: Some(2),
        ..chrome(PrivacyResource::Screen)
    };
    let state = state(vec![one, two, chrome(PrivacyResource::Camera)]);
    assert_eq!(
        sessions_for(&state, Filters::default(), "app:Google Chrome"),
        [1, 2]
    );
    assert!(!uses_microphone(&state, Filters::default()));
}

#[test]
fn the_tooltip_falls_back_to_the_title_with_no_detail_to_add() {
    let bare = usage(PrivacyResource::Location);
    assert_eq!(
        tooltip(PrivacyResource::Location, &[&bare], None),
        "Location"
    );
}

#[test]
fn the_tooltip_lists_every_known_detail() {
    let chrome = ServiceUsage {
        app: Some("chrome".to_owned()),
        ..usage(PrivacyResource::Microphone)
    };
    let discord = ServiceUsage {
        app: Some("discord".to_owned()),
        ..usage(PrivacyResource::Microphone)
    };
    assert_eq!(
        tooltip(PrivacyResource::Microphone, &[&chrome, &discord], None),
        "Microphone — chrome, discord"
    );
}

#[test]
fn a_configured_tooltip_format_replaces_the_composed_tooltip_outright() {
    let chrome = ServiceUsage {
        app: Some("chrome".to_owned()),
        ..usage(PrivacyResource::Microphone)
    };
    assert_eq!(
        tooltip(PrivacyResource::Microphone, &[&chrome], Some("Mic in use")),
        "Mic in use",
        "privacy has no per-resource token to fill, so a configured format is a plain override"
    );
}
