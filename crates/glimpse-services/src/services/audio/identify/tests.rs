use super::*;

fn props(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn stream(index: u32, props: HashMap<String, String>) -> Stream {
    Stream {
        index,
        device: DeviceId::new("sink"),
        volume: 100,
        muted: false,
        adjustable: true,
        corked: false,
        client: None,
        client_props: None,
        props,
    }
}

#[test]
fn unwraps_pipewire_alsa_name() {
    let identity = identify(
        &props(&[
            ("application.name", "PipeWire ALSA [zed-editor]"),
            ("node.name", "alsa_playback.zed-editor"),
            ("media.name", "ALSA Playback"),
        ]),
        None,
        None,
        1,
    );

    assert_eq!(identity.name, "zed-editor");
}

#[test]
fn keeps_binary_out_of_the_name_ladder() {
    let identity = identify(
        &props(&[
            ("application.name", "walz"),
            ("application.process.binary", "WebKitWebProcess"),
            ("media.role", "webaudio"),
        ]),
        None,
        None,
        2,
    );

    assert_eq!(identity.name, "walz");
    assert_eq!(identity.binary.as_deref(), Some("WebKitWebProcess"));
    assert!(!is_event_sound(&props(&[
        ("application.name", "walz"),
        ("application.process.binary", "WebKitWebProcess"),
        ("media.role", "webaudio"),
    ])));
}

#[test]
fn reads_icon_name() {
    let identity = identify(
        &props(&[
            ("application.name", "Google Chrome"),
            ("application.icon_name", "google-chrome"),
            ("application.process.binary", "chrome"),
        ]),
        None,
        None,
        3,
    );

    assert_eq!(identity.icon_name.as_deref(), Some("google-chrome"));
}

#[test]
fn event_sound_is_detected() {
    assert!(is_event_sound(&props(&[("media.role", "event")])));
}

#[test]
fn empty_props_resolve_to_an_empty_name() {
    let identity = identify(&HashMap::new(), None, None, 4);

    assert_eq!(identity.name, "");
}

#[test]
fn alsa_unwrap_survives_a_name_long_enough_to_cross_the_cap() {
    let inner = "x".repeat(60);
    let name = format!("PipeWire ALSA [{inner}]");

    let identity = identify(&props(&[("application.name", &name)]), None, None, 1);

    assert_eq!(identity.name, inner);
}

#[test]
fn application_id_is_not_capped() {
    let long_id = "x".repeat(200);

    let identity = identify(&props(&[("application.id", &long_id)]), None, None, 1);

    assert_eq!(identity.app_id.as_deref(), Some(long_id.as_str()));
    assert_eq!(identity.id, AppId::new(long_id));
}

#[test]
fn streams_without_application_id_group_by_owning_client() {
    let mut first = stream(1, HashMap::new());
    first.client = Some(99);
    let mut second = stream(2, HashMap::new());
    second.client = Some(99);

    let apps = group(vec![first, second], Vec::new());

    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].playback.as_ref().unwrap().streams.len(), 2);
}

#[test]
fn four_streams_sharing_application_id_become_one_app() {
    let make = |index: u32| stream(index, props(&[("application.id", "org.mozilla.firefox")]));

    let apps = group(vec![make(1), make(2), make(3), make(4)], Vec::new());

    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].playback.as_ref().unwrap().streams.len(), 4);
}

#[test]
fn playback_and_capture_from_same_app_merge_into_one_app() {
    let playback = stream(1, props(&[("application.id", "org.mozilla.firefox")]));
    let capture = stream(2, props(&[("application.id", "org.mozilla.firefox")]));

    let apps = group(vec![playback], vec![capture]);

    assert_eq!(apps.len(), 1);
    assert!(apps[0].playback.is_some());
    assert!(apps[0].capture.is_some());
}

#[test]
fn each_stream_keeps_its_own_device_even_when_they_differ() {
    let mut monitor = stream(1, props(&[("application.id", "chrome")]));
    monitor.device = DeviceId::new("bluez_output.monitor");
    let mut mic = stream(2, props(&[("application.id", "chrome")]));
    mic.device = DeviceId::new("mic");

    let apps = group(Vec::new(), vec![monitor, mic]);

    let streams = &apps[0].capture.as_ref().unwrap().streams;
    assert_eq!(streams.len(), 2);
    assert_eq!(
        streams.iter().map(|s| s.device.clone()).collect::<Vec<_>>(),
        vec![DeviceId::new("bluez_output.monitor"), DeviceId::new("mic")],
        "Role.device picks one arbitrary stream, but each StreamRef must keep its own so a \
             caller can tell a mixed monitor+mic app from a pure-monitor one"
    );
}

#[test]
fn long_application_name_is_capped() {
    let identity = identify(
        &props(&[("application.name", &"a".repeat(200))]),
        None,
        None,
        5,
    );

    assert_eq!(identity.name.chars().count(), NAME_CAP);
}

#[test]
fn group_is_muted_only_when_every_stream_is_muted() {
    let mut muted_stream = stream(1, props(&[("application.id", "app")]));
    muted_stream.muted = true;
    let unmuted_stream = stream(2, props(&[("application.id", "app")]));

    let apps = group(vec![muted_stream, unmuted_stream], Vec::new());

    assert_eq!(apps.len(), 1);
    assert!(!apps[0].playback.as_ref().unwrap().muted);
}

#[test]
fn missing_identity_falls_back_to_client_then_index() {
    let by_client = identify(&HashMap::new(), None, Some(42), 7);
    assert_eq!(by_client.id, AppId::new("42"));

    let by_index = identify(&HashMap::new(), None, None, 7);
    assert_eq!(by_index.id, AppId::new("7"));
}

#[test]
fn client_props_fill_gaps_left_by_the_stream() {
    let client_props = props(&[("application.icon_name", "firefox")]);
    let identity = identify(
        &props(&[("application.name", "Firefox")]),
        Some(&client_props),
        None,
        8,
    );

    assert_eq!(identity.icon_name.as_deref(), Some("firefox"));
}

#[test]
fn corked_streams_are_kept() {
    let mut corked_stream = stream(1, props(&[("application.id", "spotify")]));
    corked_stream.corked = true;

    let apps = group(vec![corked_stream], Vec::new());

    assert_eq!(apps.len(), 1);
    assert!(apps[0].playback.as_ref().unwrap().corked);
}

#[test]
fn event_sounds_are_dropped_from_grouping() {
    let event = stream(1, props(&[("media.role", "event")]));

    let apps = group(vec![event], Vec::new());

    assert!(apps.is_empty());
}

#[test]
fn result_is_sorted_by_name() {
    let zebra = stream(
        1,
        props(&[("application.name", "Zebra"), ("application.id", "z")]),
    );
    let apple = stream(
        2,
        props(&[("application.name", "Apple"), ("application.id", "a")]),
    );

    let apps = group(vec![zebra, apple], Vec::new());

    assert_eq!(apps[0].name, "Apple");
    assert_eq!(apps[1].name, "Zebra");
}
